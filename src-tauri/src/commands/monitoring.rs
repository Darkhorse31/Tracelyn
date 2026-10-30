use crate::apps::{discover_apps, load_manual_apps};
use crate::commands::companion::ensure_for_setup;
use crate::insight::otel::OtelAssembler;
use crate::setup::wizard::{
    self, interpret, summary_for, StepResult, Wizard,
};
use crate::setup::{password_bytes, privileged_read, sudo_stdin_read};
use crate::ssh::{connect_persistent, run_command_captured, SshSession};
use crate::state::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

struct Busy<'a> {
    state: &'a AppState,
    id: String,
}

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.state.end_monitoring(&self.id);
    }
}

fn publish(app: &AppHandle, server_id: &str, wizard: &Wizard) {
    let _ = app.emit(
        "monitoring-setup",
        serde_json::json!({ "serverId": server_id, "wizard": wizard }),
    );
}

fn save(app: &AppHandle, state: &AppState, server_id: &str, wizard: &Wizard) {
    state.save_monitoring(server_id, wizard.clone());
    publish(app, server_id, wizard);
}

#[tauri::command]
pub fn monitoring_setup_status(state: State<'_, AppState>, server_id: String) -> Wizard {
    state.monitoring(&server_id)
}

#[tauri::command]
pub async fn monitoring_setup_start(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
    sudo_password: Option<String>,
    apps: Option<Vec<String>>,
) -> Result<Wizard, String> {
    if let Some(password) = sudo_password {
        state.set_sudo_password(&server_id, password);
    }
    if !state.try_begin_monitoring(&server_id) {
        return Err("Setup is already running for this server.".into());
    }
    let _busy = Busy { state: &state, id: server_id.clone() };
    let mut wizard = state.monitoring(&server_id);
    let from = if wizard.pause == "needPassword" {
        if state.sudo_password(&server_id).is_none() {
            return Ok(wizard);
        }
        "access"
    } else if wizard.pause == "chooseApps" {
        let Some(chosen) = apps else {
            return Ok(wizard);
        };
        wizard.selected_apps = chosen;
        wizard.pause = "none".into();
        "apps"
    } else {
        wizard = Wizard::fresh();
        if let Some(chosen) = apps {
            wizard.selected_apps = chosen;
        }
        "access"
    };
    drive(&app, &state, &server_id, wizard, from).await
}

#[tauri::command]
pub async fn monitoring_setup_retry(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Wizard, String> {
    if !state.try_begin_monitoring(&server_id) {
        return Err("Setup is already running for this server.".into());
    }
    let _busy = Busy { state: &state, id: server_id.clone() };
    let mut wizard = state.monitoring(&server_id);
    let Some(from) = wizard.prepare_retry() else {
        return Ok(wizard);
    };
    drive(&app, &state, &server_id, wizard, &from).await
}

#[tauri::command]
pub async fn monitoring_setup_undo(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Wizard, String> {
    if !state.try_begin_monitoring(&server_id) {
        return Err("Setup is already running for this server.".into());
    }
    let _busy = Busy { state: &state, id: server_id.clone() };
    let mut wizard = state.monitoring(&server_id);
    let session = open_session(&state, &server_id).await?;
    let password = state.sudo_password(&server_id);
    let read = elevated(&session, wizard::read_manifest_script(), &wizard.access, password.as_deref()).await;
    let remote_changes = read
        .ok()
        .and_then(|(stdout, _, code)| {
            if code == 0 {
                serde_json::from_str::<Vec<crate::setup::wizard::Change>>(stdout.trim()).ok()
            } else {
                None
            }
        })
        .unwrap_or_default();
    let changes = if remote_changes.is_empty() { wizard.changes.clone() } else { remote_changes };
    if changes.is_empty() {
        wizard.summary = "Nothing has been changed yet.".into();
        save(&app, &state, &server_id, &wizard);
        return Ok(wizard);
    }
    let script = wizard::undo_script(&changes);
    let outcome = elevated(&session, &script, &wizard.access, password.as_deref()).await;
    match outcome {
        Ok((stdout, stderr, code)) => match interpret(&stdout, &stderr, code) {
            StepResult::Failed { reason, .. } => {
                wizard.summary = reason;
                save(&app, &state, &server_id, &wizard);
                Ok(wizard)
            }
            _ => {
                wizard.mark_undone();
                save(&app, &state, &server_id, &wizard);
                Ok(wizard)
            }
        },
        Err(err) => {
            wizard.summary = err;
            save(&app, &state, &server_id, &wizard);
            Ok(wizard)
        }
    }
}

async fn drive(
    app: &AppHandle,
    state: &AppState,
    server_id: &str,
    mut wizard: Wizard,
    from: &str,
) -> Result<Wizard, String> {
    let session = open_session(state, server_id).await?;
    let mut started = false;
    for (id, _) in wizard::STEP_ORDER {
        if !started {
            if *id != from {
                continue;
            }
            started = true;
        }
        if wizard.pause != "none" {
            break;
        }
        if wizard.steps.iter().any(|step| step.status == "failed") {
            break;
        }
        match *id {
            "access" => access_step(app, state, server_id, &session, &mut wizard).await?,
            "detect" => detect_step(app, state, server_id, &session, &mut wizard).await?,
            "collector" => {
                let script = wizard::collector_script(&wizard.detected.user, &wizard.detected.arch);
                script_step(app, state, server_id, &session, &mut wizard, "collector", &script).await?
            }
            "web" => {
                let nginx = wizard.detected.nginx;
                let apache = wizard.detected.apache;
                if !nginx && !apache {
                    wizard.mark("web", "skipped", "No web server was found, so timing logs were left unchanged.", "", "", "");
                    save(app, state, server_id, &wizard);
                } else {
                    let script = wizard::web_script(nginx, apache);
                    script_step(app, state, server_id, &session, &mut wizard, "web", &script).await?;
                }
            }
            "apps" => apps_step(app, state, server_id, &session, &mut wizard).await?,
            "logs" => logs_step(app, state, server_id, &session, &mut wizard).await?,
            "companion" => companion_step(app, state, server_id, &mut wizard).await?,
            "verify" => verify_step(app, state, server_id, &session, &mut wizard).await?,
            _ => {}
        }
    }
    if wizard.finished() && wizard.summary.is_empty() {
        wizard.summary = summary_for(&wizard.detected, &wizard.selected_apps);
    }
    let password = state.sudo_password(server_id);
    persist_manifest(&session, &mut wizard, password.as_deref()).await;
    save(app, state, server_id, &wizard);
    Ok(wizard)
}

async fn access_step(app: &AppHandle, state: &AppState, server_id: &str, session: &SshSession, wizard: &mut Wizard) -> Result<(), String> {
    let script = wizard::access_script();
    wizard.mark("access", "running", "", "", "", script);
    save(app, state, server_id, wizard);
    let out = run_command_captured(session, script, None).await.map_err(|e| wizard::mask_private(&e.to_string()))?;
    let (user, access) = wizard::parse_access(&out.stdout);
    if !user.is_empty() {
        wizard.detected.user = user;
    }
    wizard.access = access.clone();
    let password = state.sudo_password(server_id);
    match access.as_str() {
        "root" => wizard.mark("access", "done", "This login is the administrator.", "", "", script),
        "nopass" => wizard.mark("access", "done", "Passwordless administrator access is available.", "", "", script),
        "password" if password.is_some() => {
            wizard.mark("access", "done", "Using the administrator password for this session.", "", "", script);
        }
        "password" => {
            wizard.pause = "needPassword".into();
            wizard.mark("access", "waiting", "This server needs the administrator password to continue.", "", "", script);
        }
        _ => {
            let details = wizard::mask_private(&out.stderr);
            wizard.mark("access", "failed", "", "Could not tell whether administrator access is available.", &details, script);
        }
    }
    save(app, state, server_id, wizard);
    Ok(())
}

async fn detect_step(app: &AppHandle, state: &AppState, server_id: &str, session: &SshSession, wizard: &mut Wizard) -> Result<(), String> {
    let script = wizard::detect_script();
    wizard.mark("detect", "running", "", "", "", script);
    save(app, state, server_id, wizard);
    let password = state.sudo_password(server_id);
    match elevated(session, script, &wizard.access, password.as_deref()).await {
        Ok((stdout, stderr, code)) => {
            if code != 0 && !stdout.contains("ARCH ") {
                let details = wizard::mask_private(&format!("{stderr}\n{stdout}"));
                wizard.mark("detect", "failed", "", "Could not look at this server.", &details, script);
            } else {
                let user = wizard.detected.user.clone();
                wizard.detected = wizard::parse_detect(&stdout);
                if wizard.detected.user.is_empty() {
                    wizard.detected.user = user;
                }
                let detail = format!(
                    "Found {} Node app(s). Web server: {}.",
                    wizard.detected.nodes.len(),
                    if wizard.detected.nginx { "nginx" } else if wizard.detected.apache { "Apache" } else { "none" }
                );
                wizard.mark("detect", "done", &detail, "", "", script);
            }
        }
        Err(err) => wizard.mark("detect", "failed", "", "Could not look at this server.", &err, script),
    }
    save(app, state, server_id, wizard);
    Ok(())
}

async fn script_step(
    app: &AppHandle,
    state: &AppState,
    server_id: &str,
    session: &SshSession,
    wizard: &mut Wizard,
    id: &str,
    script: &str,
) -> Result<(), String> {
    wizard.mark(id, "running", "", "", "", script);
    save(app, state, server_id, wizard);
    let password = state.sudo_password(server_id);
    match elevated(session, script, &wizard.access, password.as_deref()).await {
        Ok((stdout, stderr, code)) => {
            wizard.add_changes(&wizard::changes_from_output(&stdout));
            match interpret(&stdout, &stderr, code) {
                StepResult::Done(detail) => wizard.mark(id, "done", &detail, "", "", script),
                StepResult::Skipped(detail) => wizard.mark(id, "skipped", &detail, "", "", script),
                StepResult::Failed { reason, details } => wizard.mark(id, "failed", "", &reason, &details, script),
            }
        }
        Err(err) => wizard.mark(id, "failed", "", "This step did not finish.", &err, script),
    }
    save(app, state, server_id, wizard);
    Ok(())
}

async fn apps_step(app: &AppHandle, state: &AppState, server_id: &str, session: &SshSession, wizard: &mut Wizard) -> Result<(), String> {
    if wizard.detected.nodes.is_empty() {
        wizard.mark(
            "apps",
            "skipped",
            "No Node app run by PM2 was found. Other app types are left unchanged.",
            "",
            "",
            "",
        );
        save(app, state, server_id, wizard);
        return Ok(());
    }
    if wizard.selected_apps.is_empty() && wizard.pause != "chooseApps" {
        let first_pass = wizard.steps.iter().find(|step| step.id == "apps").map(|step| step.status == "waiting" || step.status == "running").unwrap_or(true);
        if first_pass && wizard.steps.iter().find(|step| step.id == "apps").is_some_and(|step| step.detail.is_empty()) {
            wizard.pause = "chooseApps".into();
            wizard.mark("apps", "waiting", "Choose which Node apps should send traces.", "", "", "");
            save(app, state, server_id, wizard);
            return Ok(());
        }
    }
    let chosen: Vec<_> = wizard
        .detected
        .nodes
        .iter()
        .filter(|app| wizard.selected_apps.iter().any(|name| name == &app.name))
        .cloned()
        .collect();
    if chosen.is_empty() {
        wizard.mark("apps", "skipped", "No Node app was selected. Other app types are left unchanged.", "", "", "");
        save(app, state, server_id, wizard);
        return Ok(());
    }
    let script = wizard::node_script(&chosen);
    script_step(app, state, server_id, session, wizard, "apps", &script).await
}

async fn logs_step(app: &AppHandle, state: &AppState, server_id: &str, session: &SshSession, wizard: &mut Wizard) -> Result<(), String> {
    let script = wizard::log_probe_script();
    wizard.mark("logs", "running", "", "", "", script);
    save(app, state, server_id, wizard);
    let password = state.sudo_password(server_id);
    let manual = {
        let store = state.metrics.lock();
        load_manual_apps(store.connection(), server_id).unwrap_or_default()
    };
    match discover_apps(session, server_id, &manual, password.as_deref()).await {
        Ok(apps) => {
            let count: usize = apps.iter().map(|row| row.log_sources.len()).sum();
            state.set_apps_for_server(server_id, apps.clone());
            state.event_hub.push_apps(server_id, apps);
            if count == 0 {
                wizard.mark("logs", "failed", "", "No log files were found for this server.", "", script);
            } else {
                wizard.mark("logs", "done", &format!("{count} log source(s) are connected."), "", "", script);
            }
        }
        Err(err) => wizard.mark("logs", "failed", "", "Could not connect log files.", &wizard::mask_private(&err), script),
    }
    save(app, state, server_id, wizard);
    Ok(())
}

async fn companion_step(app: &AppHandle, state: &AppState, server_id: &str, wizard: &mut Wizard) -> Result<(), String> {
    wizard.mark("companion", "running", "", "", "", "install tracelyn-companion");
    save(app, state, server_id, wizard);
    match ensure_for_setup(app, state, server_id).await {
        Ok(detail) => wizard.mark("companion", "done", &detail, "", "", ""),
        Err(err) => wizard.mark("companion", "failed", "", &wizard::mask_private(&err), "", ""),
    }
    save(app, state, server_id, wizard);
    Ok(())
}

async fn verify_step(app: &AppHandle, state: &AppState, server_id: &str, session: &SshSession, wizard: &mut Wizard) -> Result<(), String> {
    let script = wizard::verify_script();
    wizard.mark("verify", "running", "", "", "", &script);
    save(app, state, server_id, wizard);
    let password = state.sudo_password(server_id);
    match elevated(session, &script, &wizard.access, password.as_deref()).await {
        Ok((stdout, stderr, code)) => {
            if code != 0 || !stdout.contains("VERIFY_OK") {
                wizard.mark(
                    "verify",
                    "failed",
                    "",
                    "The test trace did not show up in the collector file.",
                    &wizard::mask_private(&format!("{stderr}\n{stdout}")),
                    &script,
                );
            } else {
                let sample = stdout
                    .lines()
                    .find(|line| line.trim_start().starts_with('{'))
                    .unwrap_or("");
                let mut asm = OtelAssembler::default();
                let batch = asm.push(sample);
                if batch.spans.is_empty() {
                    wizard.mark("verify", "failed", "", "The test line was found but could not be read as a trace.", "", &script);
                } else if !stdout.contains("LOG_OK") {
                    wizard.mark("verify", "failed", "", "Traces arrived, but no log file could be read.", "", &script);
                } else {
                    wizard.summary = summary_for(&wizard.detected, &wizard.selected_apps);
                    wizard.mark("verify", "done", &wizard.summary.clone(), "", "", &script);
                }
            }
        }
        Err(err) => wizard.mark("verify", "failed", "", "Could not check that data is flowing.", &err, &script),
    }
    save(app, state, server_id, wizard);
    Ok(())
}

async fn persist_manifest(session: &SshSession, wizard: &mut Wizard, password: Option<&str>) {
    if wizard.changes.is_empty() || wizard.access.is_empty() {
        return;
    }
    let script = wizard::manifest_script(&wizard.changes);
    let _ = elevated(session, &script, &wizard.access, password).await;
}

async fn elevated(session: &SshSession, script: &str, access: &str, password: Option<&str>) -> Result<(String, String, i32), String> {
    let use_stdin = access == "password" && password.is_some();
    let command = if use_stdin { sudo_stdin_read(script) } else { privileged_read(script) };
    let stdin = if use_stdin { password.map(password_bytes) } else { None };
    let out = run_command_captured(session, &command, stdin)
        .await
        .map_err(|err| wizard::mask_private(&err.to_string()))?;
    Ok((out.stdout, wizard::mask_private(&out.stderr), out.code))
}

async fn open_session(state: &AppState, server_id: &str) -> Result<Arc<SshSession>, String> {
    if let Some(session) = state.session(server_id) {
        return Ok(session);
    }
    let server = state
        .config
        .lock()
        .get_server(server_id)
        .map_err(|err| err.to_string())?
        .clone();
    let session = connect_persistent(&server, state.host_keys.clone())
        .await
        .map_err(|err| wizard::mask_private(&err.to_string()))?;
    Ok(Arc::new(session))
}

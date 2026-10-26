use crate::ops::{
    actor_name,     excerpt, health_message, health_reached, list_audit, needs_sudo_stdin,
    operations_enabled, plan_action, probe_is_up, record, run_shell, set_operations_enabled,
    step_should_run, to_csv, AuditQuery, AuditRow, PlanInput, PlannedAction,
};
use crate::secrets::{get_secret, set_secret, SecretError, SecretKind};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, State};

fn map_err(err: impl ToString) -> String {
    err.to_string()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn mode_on(state: &AppState) -> Result<bool, String> {
    let store = state.metrics.lock();
    operations_enabled(store.connection()).map_err(map_err)
}

fn read_only_error() -> String {
    "Operations mode is off. The app stays read-only until you turn it on in Settings.".into()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionRequest {
    pub server_id: String,
    pub kind: String,
    pub verb: String,
    pub target: String,
    #[serde(default)]
    pub pm2_user: Option<String>,
    #[serde(default)]
    pub pm2_home: Option<String>,
    #[serde(default)]
    pub log_paths: Vec<String>,
    #[serde(default)]
    pub use_sudo: bool,
    #[serde(default)]
    pub confirm_name: Option<String>,
    #[serde(default)]
    pub health_timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionPreview {
    pub command: String,
    pub dangerous: bool,
    pub confirm_name: String,
    pub pm2_user: Option<String>,
    pub pm2_home: Option<String>,
    pub use_sudo: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionOutcome {
    pub audit_id: String,
    pub command: String,
    pub exit_code: i32,
    pub output_excerpt: String,
    pub health_ok: Option<bool>,
    pub health_message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditFilter {
    #[serde(default)]
    pub server_id: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub since_ms: Option<i64>,
    #[serde(default)]
    pub until_ms: Option<i64>,
    #[serde(default)]
    pub failed_only: bool,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SudoStatus {
    pub configured: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermOpen {
    pub server_id: String,
    pub cols: u32,
    pub rows: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermOpened {
    pub tab_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermWrite {
    pub tab_id: String,
    pub data: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermResize {
    pub tab_id: String,
    pub cols: u32,
    pub rows: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermClose {
    pub tab_id: String,
}

fn sudo_configured(server_id: &str) -> bool {
    matches!(get_secret(server_id, SecretKind::SudoPassword), Ok(_))
}

fn plan_from(state: &AppState, request: &ActionRequest, require_confirm: bool) -> Result<(crate::config::ServerRecord, PlannedAction), String> {
    let server = {
        let config = state.config.lock();
        config.get_server(&request.server_id).map_err(map_err)?.clone()
    };
    let planned = plan_action(
        &PlanInput {
            kind: request.kind.clone(),
            verb: request.verb.clone(),
            target: request.target.clone(),
            ssh_user: server.username.clone(),
            pm2_user: request.pm2_user.clone(),
            pm2_home: request.pm2_home.clone(),
            log_paths: request.log_paths.clone(),
            use_sudo: request.use_sudo,
            sudo_password_set: sudo_configured(&server.id),
            confirm_name: request.confirm_name.clone(),
            health_timeout_secs: request.health_timeout_secs,
        },
        require_confirm,
    )?;
    Ok((server, planned))
}

fn audit_query(filter: &AuditFilter) -> AuditQuery {
    AuditQuery {
        server_id: filter.server_id.clone().unwrap_or_default(),
        target: filter.target.clone().unwrap_or_default(),
        since_ms: filter.since_ms.unwrap_or(0),
        until_ms: filter.until_ms.unwrap_or(0),
        failed_only: filter.failed_only,
        limit: filter.limit.unwrap_or(200) as i64,
    }
}

fn write_audit(
    state: &AppState,
    server_id: &str,
    server_name: &str,
    target: &str,
    command: &str,
    exit_code: i32,
    output: &str,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let row = AuditRow {
        id: id.clone(),
        actor: actor_name(),
        server_id: server_id.to_string(),
        server_name: server_name.to_string(),
        target: target.to_string(),
        command: command.to_string(),
        ts_ms: now_ms(),
        exit_code: Some(exit_code),
        output_excerpt: excerpt(output),
    };
    let store = state.metrics.lock();
    record(store.connection(), &row).map_err(map_err)?;
    Ok(id)
}

#[tauri::command]
pub fn ops_get_mode(state: State<AppState>) -> Result<bool, String> {
    mode_on(&state)
}

#[tauri::command]
pub async fn ops_set_mode(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    {
        let store = state.metrics.lock();
        set_operations_enabled(store.connection(), enabled).map_err(map_err)?;
    }
    if !enabled {
        state.ops.close_all().await;
    }
    Ok(())
}

#[tauri::command]
pub fn ops_set_sudo_password(server_id: String, password: String) -> Result<(), String> {
    if password.is_empty() {
        return Err("Sudo password cannot be empty.".into());
    }
    set_secret(&server_id, SecretKind::SudoPassword, &password).map_err(map_err)
}

#[tauri::command]
pub fn ops_sudo_configured(server_id: String) -> Result<SudoStatus, String> {
    match get_secret(&server_id, SecretKind::SudoPassword) {
        Ok(_) => Ok(SudoStatus { configured: true }),
        Err(SecretError::NotFound(_)) => Ok(SudoStatus { configured: false }),
        Err(err) => Err(map_err(err)),
    }
}

#[tauri::command]
pub fn ops_preview(state: State<AppState>, request: ActionRequest) -> Result<ActionPreview, String> {
    let (_server, planned) = plan_from(&state, &request, false)?;
    Ok(ActionPreview {
        command: planned.display(),
        dangerous: planned.dangerous,
        confirm_name: planned.confirm_name,
        pm2_user: planned.pm2_user,
        pm2_home: planned.pm2_home,
        use_sudo: planned.use_sudo,
    })
}

#[tauri::command]
pub async fn ops_run(
    state: State<'_, AppState>,
    request: ActionRequest,
) -> Result<ActionOutcome, String> {
    if !mode_on(&state)? {
        return Err(read_only_error());
    }
    let (server, planned) = plan_from(&state, &request, true)?;
    let host_keys = state.host_keys.clone();
    let session = match state.ops.session_for(&server, host_keys).await {
        Ok(session) => session,
        Err(err) => {
            let audit_id = write_audit(
                &state,
                &server.id,
                &server.name,
                &planned.target,
                &planned.display(),
                -1,
                &err,
            )?;
            return Ok(ActionOutcome {
                audit_id,
                command: planned.display(),
                exit_code: -1,
                output_excerpt: excerpt(&err),
                health_ok: Some(false),
                health_message: err,
            });
        }
    };

    let wants_secret = planned.steps.iter().any(|step| needs_sudo_stdin(&step.command))
        || needs_sudo_stdin(&planned.health_command);
    let password = if wants_secret {
        match get_secret(&server.id, SecretKind::SudoPassword) {
            Ok(value) => Some(value),
            Err(SecretError::NotFound(_)) => None,
            Err(err) => return Err(map_err(err)),
        }
    } else {
        None
    };

    let mut combined = String::new();
    let mut exit_code = 0;
    let mut prev_failed = false;
    let mut skipped = false;
    for step in &planned.steps {
        if !step_should_run(prev_failed, step.skip_if_prev_failed) {
            skipped = true;
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str("(reload was not run because the previous command failed)");
            break;
        }
        match run_shell(
            &session,
            &step.command,
            password.as_deref(),
            Duration::from_secs(30),
        )
        .await
        {
            Ok(out) => {
                exit_code = out.exit_code;
                prev_failed = exit_code != 0;
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(&out.output);
            }
            Err(err) => {
                exit_code = -1;
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(&err);
                break;
            }
        }
    }

    let (health_ok, health_message) = if skipped {
        (
            Some(false),
            "Config test failed, so reload was not run.".to_string(),
        )
    } else {
        let started = Instant::now();
        let limit = Duration::from_secs(planned.health_timeout_secs);
        let mut reached = false;
        loop {
            let up = match run_shell(
                &session,
                &planned.health_command,
                password.as_deref(),
                Duration::from_secs(5),
            )
            .await
            {
                Ok(out) => probe_is_up(&planned.kind, &planned.target, &out.output),
                Err(_) => false,
            };
            if health_reached(&planned.verb, up) {
                reached = true;
                break;
            }
            if started.elapsed() >= limit {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        (
            Some(reached),
            health_message(
                &planned.verb,
                reached,
                started.elapsed().as_secs(),
                planned.health_timeout_secs,
            ),
        )
    };

    let audit_id = write_audit(
        &state,
        &server.id,
        &server.name,
        &planned.target,
        &planned.display(),
        exit_code,
        &combined,
    )?;
    Ok(ActionOutcome {
        audit_id,
        command: planned.display(),
        exit_code,
        output_excerpt: excerpt(&combined),
        health_ok,
        health_message,
    })
}

#[tauri::command]
pub fn ops_list_audit(
    state: State<AppState>,
    filter: AuditFilter,
) -> Result<Vec<AuditRow>, String> {
    let store = state.metrics.lock();
    list_audit(store.connection(), &audit_query(&filter)).map_err(map_err)
}

#[tauri::command]
pub fn ops_export_audit(state: State<AppState>, filter: AuditFilter) -> Result<String, String> {
    let store = state.metrics.lock();
    let rows = list_audit(store.connection(), &audit_query(&filter)).map_err(map_err)?;
    Ok(to_csv(&rows))
}

#[tauri::command]
pub async fn ops_terminal_open(
    app: AppHandle,
    state: State<'_, AppState>,
    request: TermOpen,
) -> Result<TermOpened, String> {
    if !mode_on(&state)? {
        return Err(read_only_error());
    }
    let server = {
        let config = state.config.lock();
        config.get_server(&request.server_id).map_err(map_err)?.clone()
    };
    let tab_id = state
        .ops
        .open_terminal(app, &server, state.host_keys.clone(), request.cols, request.rows)
        .await?;
    Ok(TermOpened { tab_id })
}

#[tauri::command]
pub async fn ops_terminal_write(state: State<'_, AppState>, request: TermWrite) -> Result<(), String> {
    if !mode_on(&state)? {
        return Err(read_only_error());
    }
    state.ops.write_tab(&request.tab_id, &request.data).await
}

#[tauri::command]
pub async fn ops_terminal_resize(
    state: State<'_, AppState>,
    request: TermResize,
) -> Result<(), String> {
    if !mode_on(&state)? {
        return Err(read_only_error());
    }
    state
        .ops
        .resize_tab(&request.tab_id, request.cols, request.rows)
        .await
}

#[tauri::command]
pub async fn ops_terminal_close(state: State<'_, AppState>, request: TermClose) -> Result<(), String> {
    state.ops.close_tab(&request.tab_id).await
}

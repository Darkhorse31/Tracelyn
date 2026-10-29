//! Optional companion management. Preview and execution use the same plan.
use super::ops::{ActionOutcome, ActionPreview, ActionRequest};
use crate::{
    ops::{actor_name, record, run_shell, AuditRow},
    state::AppState,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Duration};
use tauri::{AppHandle, Manager, State};
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn server(state: &AppState, id: &str) -> Result<crate::config::ServerRecord, String> {
    state
        .config
        .lock()
        .get_server(id)
        .cloned()
        .map_err(|e| e.to_string())
}
fn enabled(state: &AppState) -> Result<(), String> {
    crate::license::require_write()?;
    if !crate::ops::operations_enabled(state.metrics.lock().connection())
        .map_err(|e| e.to_string())?
    {
        return Err("Enable Operations mode in Settings to manage the companion".into());
    }
    Ok(())
}
pub async fn remote(state: &AppState, id: &str, args: &str) -> Result<String, String> {
    if !matches!(
        state.runtime(id).status,
        crate::state::ConnectionStatus::Connected | crate::state::ConnectionStatus::Degraded
    ) {
        return Err("Connect the server before reading companion data".into());
    }
    let s = server(state, id)?;
    let session = state.ops.session_for(&s, state.host_keys.clone()).await?;
    let guard = session.lock().await;
    tokio::time::timeout(
        Duration::from_secs(20),
        crate::ssh::run_command(&guard, args),
    )
    .await
    .map_err(|_| "Companion request timed out".to_string())?
    .map_err(|_| "Companion unavailable or access denied".to_string())
}
#[tauri::command]
pub async fn companion_status(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Value, String> {
    let out=remote(&state,&server_id,r#"if test -x /usr/local/bin/tracelyn-companion
then
if systemctl is-active --quiet tracelyn-companion
then /usr/local/bin/tracelyn-companion status --json
else
heartbeat=$(stat -c %Y /var/lib/tracelyn/heartbeat 2>/dev/null || printf 0)
disk=$(du -sb /var/lib/tracelyn 2>/dev/null | cut -f1)
version=$(/usr/local/bin/tracelyn-companion version)
printf '{"state":"stopped","version":"%s","uptimeSecs":0,"lastHeartbeat":%s,"diskUsed":%s}' "$version" "$((heartbeat * 1000))" "${disk:-0}"
fi
else printf '{"state":"not installed"}'
fi"#).await?;
    let mut value: Value =
        serde_json::from_str(out.trim()).map_err(|_| "Could not read companion status")?;
    if value["state"] == "running" && value["version"] != tracelyn_companion::VERSION {
        value["state"] = json!("outdated");
    }
    Ok(value)
}
struct Plan {
    command: String,
    uploads: Vec<(String, Vec<u8>)>,
    stage: String,
}
fn cleanup(stage: &str) -> String {
    format!("rm -f {stage}/binary {stage}/unit {stage}/rules.json {stage}/channels.json {stage}/watch.json\nrmdir {stage}")
}
fn resource(app: &AppHandle, arch: &str) -> Result<Vec<u8>, String> {
    let name = match arch.trim() {
        "x86_64" => "x86_64-unknown-linux-musl",
        "aarch64" | "arm64" => "aarch64-unknown-linux-musl",
        _ => return Err("Companion supports x86_64 and aarch64 Linux servers".into()),
    };
    let filename = format!("tracelyn-companion-{name}");
    let base = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("companion");
    let mut path = base.join(&filename);
    if !path.exists() {
        path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/companion")
            .join(&filename);
    }
    let bytes=std::fs::read(&path).map_err(|_|format!("Companion binary is not bundled for {name}. Build it using scripts/build-companion.ps1 and rebuild the desktop app"))?;
    let expected = std::fs::read_to_string(path.with_extension("sha256"))
        .map_err(|_| "Companion checksum is missing")?;
    let hash = hex::encode(Sha256::digest(&bytes));
    if expected.split_whitespace().next() != Some(hash.as_str()) {
        return Err("Bundled companion checksum does not match".into());
    }
    Ok(bytes)
}
fn configs(
    state: &AppState,
    id: &str,
    paths: Option<&[String]>,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    use crate::intel::{notify::get_channel_secret, store};
    let db = state.metrics.lock();
    let conn = db.connection();
    crate::intel::migrate(conn).map_err(|e| e.to_string())?;
    store::seed_defaults(conn, crate::intel::now_ms()).map_err(|e| e.to_string())?;
    let mut rules = store::list_rules(conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|r| r.server_id.as_deref().map(|s| s == id).unwrap_or(true))
        .map(|mut r| {
            r.firing = false;
            r
        })
        .collect::<Vec<_>>();
    for r in crate::alerts::list_rules(conn).map_err(|e| e.to_string())? {
        if r.server_id.as_deref().map(|s| s != id).unwrap_or(false) {
            continue;
        }
        let metric = match r.kind.as_str() {
            "cpu_high" => "cpu.percent",
            "memory_high" => "memory.percent",
            "disk_high" => "disk.percent",
            _ => continue,
        };
        rules.push(tracelyn_core::types::IntelRule {
            id: r.id,
            server_id: r.server_id,
            kind: "metric".into(),
            name: r.name,
            enabled: r.enabled,
            severity: "critical".into(),
            params: tracelyn_core::types::RuleParams {
                threshold: r.params.threshold,
                clear_threshold: Some(
                    r.params
                        .clear_threshold
                        .unwrap_or(r.params.threshold * 0.95)
                        .max(r.params.threshold),
                ),
                duration_secs: r.params.duration_secs,
                metric_key: r.params.metric_key.unwrap_or_else(|| metric.into()),
                ..Default::default()
            },
            created_at_ms: r.created_at_ms,
            updated_at_ms: r.updated_at_ms,
            firing: false,
        });
    }
    let channels = store::list_channels(conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|c| c.kind != "windows")
        .map(|c| tracelyn_companion::notify::ServerChannel {
            secret: get_channel_secret(&c.id).unwrap_or_default(),
            channel: c,
        })
        .collect::<Vec<_>>();
    let key = format!("companion_watch:{id}");
    let saved: String = conn
        .query_row("SELECT value FROM app_settings WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .unwrap_or_else(|_| "[]".into());
    let saved: Vec<String> = serde_json::from_str(&saved).unwrap_or_default();
    let paths = paths.unwrap_or(&saved);
    if paths.len() > 32
        || paths
            .iter()
            .any(|p| !p.starts_with('/') || p.len() > 4096 || p.contains(['\n', '\r', '\0']))
    {
        return Err("Use at most 32 absolute Linux log paths".into());
    }
    let watch =
        json!({"files":paths,"utcOffsetSecs":chrono::Local::now().offset().local_minus_utc()});
    Ok(vec![
        (
            "rules.json".into(),
            serde_json::to_vec(&rules).map_err(|e| e.to_string())?,
        ),
        (
            "channels.json".into(),
            serde_json::to_vec(&channels).map_err(|e| e.to_string())?,
        ),
        (
            "watch.json".into(),
            serde_json::to_vec(&watch).map_err(|e| e.to_string())?,
        ),
    ])
}
pub(crate) fn rules_hash(state: &AppState, id: &str) -> Result<String, String> {
    let files = configs(state, id, None)?;
    let mut hash = Sha256::new();
    for (_, bytes) in files {
        hash.update(bytes);
    }
    Ok(hex::encode(hash.finalize()))
}
#[tauri::command]
pub fn companion_log_paths(
    state: State<AppState>,
    server_id: String,
) -> Result<Vec<String>, String> {
    server(&state, &server_id)?;
    let key = format!("companion_watch:{server_id}");
    let value: String = state
        .metrics
        .lock()
        .connection()
        .query_row("SELECT value FROM app_settings WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .unwrap_or_else(|_| "[]".into());
    serde_json::from_str(&value).map_err(|e| e.to_string())
}
pub(crate) async fn sync_one(app: &AppHandle, state: &AppState, id: &str) -> Result<(), String> {
    let request = ActionRequest {
        server_id: id.into(),
        kind: "companion".into(),
        verb: "sync".into(),
        target: "tracelyn-companion".into(),
        pm2_user: None,
        pm2_home: None,
        log_paths: Vec::new(),
        use_sudo: true,
        confirm_name: Some("tracelyn-companion".into()),
        health_timeout_secs: None,
    };
    execute(app, state, request).await.map(|_| ())
}
async fn plan(app: &AppHandle, state: &AppState, r: &ActionRequest) -> Result<Plan, String> {
    let s = server(state, &r.server_id)?;
    let stage = format!(
        "/tmp/tracelyn-{}",
        hex::encode(Sha256::digest(s.id.as_bytes()))
    );
    let mut uploads = Vec::new();
    let mut commands = Vec::<String>::new();
    match r.verb.as_str() {
        "install"|"update"=>{
            let arch=remote(state,&s.id,"uname -m").await?;let bytes=resource(app,&arch)?;
            let hash=hex::encode(Sha256::digest(&bytes));uploads.push(("binary".into(),bytes));
            commands.extend([
                "getent group tracelyn >/dev/null || groupadd --system tracelyn".into(),
                "id tracelyn >/dev/null 2>&1 || useradd --system --gid tracelyn --home-dir /var/lib/tracelyn --shell /usr/sbin/nologin tracelyn".into(),
                "test \"$(id -u tracelyn)\" -ne 0".into(),
                "getent group adm >/dev/null || groupadd --system adm".into(),
                "getent group systemd-journal >/dev/null || groupadd --system systemd-journal".into(),
                "usermod -a -G adm,systemd-journal tracelyn".into(),
                "install -d -o tracelyn -g tracelyn -m 0750 /var/lib/tracelyn /run/tracelyn /etc/tracelyn".into(),
                format!("printf '%s  %s\\n' {} {} | sha256sum -c -",quote(&hash),quote(&format!("{stage}/binary"))),
                format!("install -o root -g root -m 0755 {stage}/binary /usr/local/bin/tracelyn-companion.next"),
                "mv /usr/local/bin/tracelyn-companion.next /usr/local/bin/tracelyn-companion".into(),
                format!("install -o root -g root -m 0644 {stage}/unit /etc/systemd/system/tracelyn-companion.service"),
            ]);
            if s.username != "root" { commands.push(format!("usermod -a -G tracelyn -- {}", quote(&s.username))); }
            uploads.push(("unit".into(),tracelyn_companion::systemd_unit().into_bytes()));
        },
        "restart"|"stop"=>commands.push(format!("systemctl {} tracelyn-companion",r.verb)),
        "uninstall"=>commands.extend(["systemctl disable --now tracelyn-companion".into(),"rm -f /etc/systemd/system/tracelyn-companion.service /usr/local/bin/tracelyn-companion".into(),"systemctl daemon-reload".into()]),
        "sync"|"configure"=>{},
        _=>return Err("Unknown companion action".into())
    }
    if matches!(r.verb.as_str(), "install" | "update" | "sync" | "configure") {
        for (name, bytes) in configs(
            state,
            &s.id,
            if r.verb == "configure" {
                Some(&r.log_paths)
            } else {
                None
            },
        )? {
            let hash = hex::encode(Sha256::digest(&bytes));
            commands.push(format!(
                "printf '%s  %s\\n' {} {} | sha256sum -c -",
                quote(&hash),
                quote(&format!("{stage}/{name}"))
            ));
            commands.push(format!(
                "install -o tracelyn -g tracelyn -m 0600 {stage}/{name} /etc/tracelyn/{name}.next"
            ));
            commands.push(format!("mv /etc/tracelyn/{name}.next /etc/tracelyn/{name}"));
            uploads.push((name, bytes));
        }
        if matches!(r.verb.as_str(), "install" | "update") {
            commands.push("systemctl daemon-reload".into());
            commands.push("systemctl enable tracelyn-companion".into());
            commands.push("systemctl restart tracelyn-companion".into());
        }
    }
    if matches!(r.verb.as_str(), "install" | "update" | "restart") {
        commands.push("for attempt in 1 2 3 4 5 6 7 8 9 10\ndo\nif systemctl is-active --quiet tracelyn-companion && test -S /run/tracelyn/companion.sock\nthen\n/usr/local/bin/tracelyn-companion status --json\nbreak\nfi\nsleep 1\ndone\nsystemctl is-active --quiet tracelyn-companion\ntest -S /run/tracelyn/companion.sock".into());
    }
    // set -e is inside the privileged shell so failed verification prevents install.
    let script = format!("set -eu\n{}", commands.join("\n"));
    let sudo = if s.username == "root" {
        ""
    } else if crate::secrets::get_secret(&s.id, crate::secrets::SecretKind::SudoPassword).is_ok() {
        "sudo -S -p '' "
    } else {
        "sudo -n "
    };
    let command = format!("{sudo}sh -c {}", quote(&script));
    Ok(Plan {
        command,
        uploads,
        stage,
    })
}
#[tauri::command]
pub async fn companion_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ActionRequest,
) -> Result<ActionPreview, String> {
    let p = plan(&app, &state, &request).await?;
    let upload = if p.uploads.is_empty() {
        String::new()
    } else {
        format!(
            "umask 077\nmkdir -m 700 {}\n# SFTP upload: {}\n",
            p.stage,
            p.uploads
                .iter()
                .map(|(n, _)| format!("{}/{n}", p.stage))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Ok(ActionPreview {
        command: if upload.is_empty() {
            p.command
        } else {
            format!("{upload}{}\n{}", p.command, cleanup(&p.stage))
        },
        dangerous: true,
        confirm_name: "tracelyn-companion".into(),
        pm2_user: None,
        pm2_home: None,
        use_sudo: true,
    })
}
#[tauri::command]
pub async fn companion_run(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ActionRequest,
) -> Result<ActionOutcome, String> {
    execute(&app, &state, request).await
}
async fn execute(
    app: &AppHandle,
    state: &AppState,
    request: ActionRequest,
) -> Result<ActionOutcome, String> {
    enabled(&state)?;
    if !matches!(
        state.runtime(&request.server_id).status,
        crate::state::ConnectionStatus::Connected | crate::state::ConnectionStatus::Degraded
    ) {
        return Err("Connect the server before managing its companion".into());
    }
    if request.confirm_name.as_deref() != Some("tracelyn-companion") {
        return Err("Type tracelyn-companion to confirm".into());
    }
    let p = plan(&app, &state, &request).await?;
    let s = server(&state, &request.server_id)?;
    let sudo_password = if crate::ops::needs_sudo_stdin(&p.command) {
        Some(
            crate::secrets::get_secret(&s.id, crate::secrets::SecretKind::SudoPassword)
                .map_err(|_| "Save the sudo password in Settings and retry")?,
        )
    } else {
        None
    };
    let id = uuid::Uuid::new_v4().to_string();
    let timestamp = crate::intel::now_ms();
    let audit = |exit_code, output: &str| {
        record(
            state.metrics.lock().connection(),
            &AuditRow {
                id: id.clone(),
                actor: actor_name(),
                server_id: s.id.clone(),
                server_name: s.name.clone(),
                target: "tracelyn-companion".into(),
                command: p.command.clone(),
                ts_ms: timestamp,
                exit_code,
                output_excerpt: output.into(),
            },
        )
        .map_err(|e| e.to_string())
    };
    audit(None, "Started")?;
    let mut staging_session = None;
    let mut created = false;
    let result=async {
        let session=state.ops.session_for(&s,state.host_keys.clone()).await?;
        if !p.uploads.is_empty(){
            let out=run_shell(&session,&format!("umask 077\nmkdir -m 700 {}",p.stage),None,Duration::from_secs(10)).await?;
            if out.exit_code!=0{return Err("Could not create private staging directory. Remove the stale companion staging directory shown in the preview and retry".into());}
            created=true;
            staging_session=Some(session.clone());
            for(name,bytes)in &p.uploads {
                tokio::time::timeout(Duration::from_secs(120),async {
                    session.lock().await.upload_companion_file(&format!("{}/{name}",p.stage),bytes).await
                }).await.map_err(|_|"Companion upload timed out".to_string())??;
            }
        }
        enabled(&state)?;
        let out=run_shell(&session,&p.command,sudo_password.as_deref(),Duration::from_secs(60)).await?;
        if out.exit_code!=0{return Err("Companion action failed. Save the sudo password in Settings or configure access with sudo visudo. Check service logs with sudo journalctl -u tracelyn-companion -n 50 --no-pager".into());}Ok::<_,String>(out)
    }.await;
    // Clean private uploads after both success and failure. Never remove a
    // directory that this operation did not create.
    if created {
        if let Some(session) = staging_session {
            let _ = run_shell(&session, &cleanup(&p.stage), None, Duration::from_secs(10)).await;
        }
    }
    match result {
        Ok(out) => {
            if matches!(request.verb.as_str(), "stop" | "uninstall") {
                crate::companion_history::forget(&s.id);
            }
            if request.verb == "configure" {
                state.metrics.lock().connection().execute("INSERT INTO app_settings(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",rusqlite::params![format!("companion_watch:{}",s.id),serde_json::to_string(&request.log_paths).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
            }
            state
                .metrics
                .lock()
                .connection()
                .execute(
                    "UPDATE audit_log SET exit_code=0,output_excerpt='Completed' WHERE id=?1",
                    [&id],
                )
                .map_err(|e| e.to_string())?;
            if request.verb == "install" {
                state.ops.drop_server(&s.id).await;
            }
            Ok(ActionOutcome {
                audit_id: id.clone(),
                command: format!("companion {}", request.verb),
                exit_code: out.exit_code,
                output_excerpt: "Completed".into(),
                health_ok: None,
                health_message: "Companion action completed".into(),
            })
        }
        Err(e) => {
            state
                .metrics
                .lock()
                .connection()
                .execute(
                    "UPDATE audit_log SET exit_code=-1,output_excerpt=?1 WHERE id=?2",
                    rusqlite::params![e, id],
                )
                .map_err(|e| e.to_string())?;
            Err(e)
        }
    }
}
#[tauri::command]
pub async fn companion_query(
    state: State<'_, AppState>,
    server_id: String,
    since: i64,
    until: i64,
    metric: String,
) -> Result<Value, String> {
    if metric.len() > 100
        || metric
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || "._-".contains(c)))
    {
        return Err("Invalid metric".into());
    }
    let command=format!("/usr/local/bin/tracelyn-companion query --since {since} --until {until} --metric {} --json",quote(&metric));
    serde_json::from_str(&remote(&state, &server_id, &command).await?)
        .map_err(|_| "Invalid companion history".into())
}
#[tauri::command]
pub async fn companion_sync_all(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let ids = state
        .config
        .lock()
        .list_servers()
        .iter()
        .map(|s| s.id.clone())
        .collect::<Vec<_>>();
    let mut failures = 0;
    for id in ids {
        if !matches!(
            state.runtime(&id).status,
            crate::state::ConnectionStatus::Connected | crate::state::ConnectionStatus::Degraded
        ) {
            continue;
        }
        let exists = remote(
            &state,
            &id,
            "test -x /usr/local/bin/tracelyn-companion && printf installed",
        )
        .await;
        if exists.as_deref() != Ok("installed") {
            continue;
        }
        let request = ActionRequest {
            server_id: id,
            kind: "companion".into(),
            verb: "sync".into(),
            target: "tracelyn-companion".into(),
            pm2_user: None,
            pm2_home: None,
            log_paths: Vec::new(),
            use_sudo: true,
            confirm_name: Some("tracelyn-companion".into()),
            health_timeout_secs: None,
        };
        if execute(&app, &state, request).await.is_err() {
            failures += 1;
        }
    }
    if failures > 0 {
        return Err(format!("Saved locally. Configuration sync failed on {failures} companion servers. Open Server companion and retry Sync configuration"));
    }
    Ok(())
}

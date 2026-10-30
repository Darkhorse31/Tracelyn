use crate::server_import::{parse_csv, parse_ssh_config, ImportPreview};
use crate::state::AppState;
use serde::Serialize;
use std::time::Duration;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    imported: Vec<crate::config::ServerRecord>,
    skipped: usize,
}

#[tauri::command]
pub fn import_server_profiles(
    state: State<AppState>,
    inputs: Vec<super::UpsertServerInput>,
) -> Result<ImportOutcome, String> {
    use crate::config::{AuthType, ServerRecord};
    if inputs.len() > 5000 {
        return Err("Import at most 5000 profiles at a time".into());
    }
    let endpoint = |host: &str, port: u16, user: &str| {
        (
            host.trim()
                .trim_matches(['[', ']'])
                .trim_end_matches('.')
                .to_ascii_lowercase(),
            port,
            user.trim().to_string(),
        )
    };
    let mut config = state.config.lock();
    let mut seen: std::collections::HashSet<_> = config
        .list_servers()
        .iter()
        .map(|s| endpoint(&s.host, s.port, &s.username))
        .collect();
    let mut imported = Vec::new();
    let mut skipped = 0;
    for input in inputs {
        if input.name.trim().is_empty()
            || input.host.trim().is_empty()
            || input.host.chars().any(char::is_whitespace)
            || input.host.starts_with('-')
            || input.username.trim().is_empty()
            || input.username.chars().any(char::is_whitespace)
            || input.port == 0
        {
            return Err(
                "Every imported profile needs a name, host, username, and port from 1 to 65535"
                    .into(),
            );
        }
        if input.password.is_some() || input.key_passphrase.is_some() {
            return Err("Add credentials in Edit server after importing".into());
        }
        if !seen.insert(endpoint(&input.host, input.port, &input.username)) {
            skipped += 1;
            continue;
        }
        imported.push(ServerRecord {
            id: uuid::Uuid::new_v4().to_string(),
            name: input.name.trim().into(),
            host: input.host.trim().trim_matches(['[', ']']).into(),
            port: input.port,
            username: input.username.trim().into(),
            auth_type: if input.auth_type == AuthType::Key {
                AuthType::Key
            } else {
                AuthType::Password
            },
            key_path: input.key_path,
            labels: input.labels,
        });
    }
    config
        .import_servers(imported.clone())
        .map_err(|e| e.to_string())?;
    Ok(ImportOutcome { imported, skipped })
}

#[tauri::command]
pub fn export_server_profiles(state: State<AppState>, path: String) -> Result<(), String> {
    let config = state.config.lock();
    let csv = crate::server_import::export_csv(config.list_servers());
    std::fs::write(path, csv).map_err(|e| format!("Could not export server profiles: {e}"))
}

#[tauri::command]
pub fn preview_server_import(
    format: String,
    text: Option<String>,
) -> Result<ImportPreview, String> {
    if text.as_ref().is_some_and(|t| t.len() > 2_000_000) {
        return Err("Import files must be smaller than 2 MB".into());
    }
    match format.as_str() {
        "ssh" => {
            let home = dirs::home_dir().ok_or("Home directory is unavailable")?;
            let raw = match text {
                Some(raw) => raw,
                None => {
                    let path = home.join(".ssh").join("config");
                    let metadata = std::fs::metadata(&path).map_err(|_| {
                        "No ~/.ssh/config found. Choose a file or paste Host entries"
                    })?;
                    if metadata.len() > 2_000_000 {
                        return Err("Import files must be smaller than 2 MB".into());
                    }
                    std::fs::read_to_string(path).map_err(|e| e.to_string())?
                }
            };
            let user = std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_default();
            Ok(parse_ssh_config(&raw, &home.to_string_lossy(), &user))
        }
        "csv" => parse_csv(text.as_deref().unwrap_or("")),
        _ => Err("Choose SSH config or CSV".into()),
    }
}

/// Separate TCP check lets onboarding report real progress without simulated timers.
#[tauri::command]
pub async fn probe_server_port(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<(), String> {
    let server = state
        .config
        .lock()
        .get_server(&server_id)
        .map_err(|e| e.to_string())?
        .clone();
    tokio::time::timeout(
        Duration::from_secs(12),
        tokio::net::TcpStream::connect((server.host.as_str(), server.port)),
    )
    .await
    .map_err(|_| "Connection timed out. Check the host, port, firewall, and VPN".to_string())?
    .map_err(|e| format!("Could not reach SSH port: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn test_onboarding_connection(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<crate::ssh::ConnectionTestResult, String> {
    let server = state
        .config
        .lock()
        .get_server(&server_id)
        .map_err(|e| e.to_string())?
        .clone();
    tokio::time::timeout(
        Duration::from_secs(35),
        crate::ssh::test_connection(&server, None, None, state.host_keys.clone()),
    )
    .await
    .map_err(|_| "SSH authentication timed out. Check your key, credentials, and VPN".to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn run_onboarding_setup_checks(
    state: State<'_, AppState>,
    infra: State<'_, crate::infra::InfraState>,
    server_id: String,
) -> Result<Vec<crate::setup::SetupCheck>, String> {
    tokio::time::timeout(
        Duration::from_secs(60),
        super::setup::run_setup_checks(state, infra, server_id),
    )
    .await
    .map_err(|_| {
        "Setup checks timed out. Retry or continue and use the Setup Guide later".to_string()
    })?
}

#[tauri::command]
pub async fn start_onboarding_monitoring(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    server_id: String,
) -> Result<(), String> {
    let server = state
        .config
        .lock()
        .get_server(&server_id)
        .map_err(|e| e.to_string())?
        .clone();
    state.connections.connect(app, server).await?;
    for _ in 0..60 {
        let runtime = state.runtime(&server_id);
        if runtime.status == crate::state::ConnectionStatus::Connected {
            return Ok(());
        }
        if let Some(error) = runtime.last_error {
            return Err(error);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err("Monitoring is still connecting. Retry or check the server status on the dashboard".into())
}

#[tauri::command]
pub async fn discover_onboarding_apps(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<crate::apps::TracelynApp>, String> {
    let server = state
        .config
        .lock()
        .get_server(&server_id)
        .map_err(|e| e.to_string())?
        .clone();
    let manual = {
        let store = state.metrics.lock();
        crate::apps::load_manual_apps(store.connection(), &server_id).map_err(|e| e.to_string())?
    };
    let session = tokio::time::timeout(
        Duration::from_secs(20),
        crate::ssh::connect_persistent(&server, state.host_keys.clone()),
    )
    .await
    .map_err(|_| "SSH connection timed out")?
    .map_err(|e| e.to_string())?;
    // Discover hidden apps too, so reopening the wizard can restore their visibility.
    let visible_overrides: Vec<_> = manual.iter().filter(|a| !a.hidden).cloned().collect();
    let sudo_password = state.sudo_password(&server_id);
    let result = tokio::time::timeout(
        Duration::from_secs(45),
        crate::apps::discover_apps(
            &session,
            &server_id,
            &visible_overrides,
            sudo_password.as_deref(),
        ),
    )
    .await;
    let _ = session
        .disconnect(russh::Disconnect::ByApplication, "", "English")
        .await;
    let mut apps =
        result.map_err(|_| "App discovery timed out. Retry or continue with server metrics")??;
    for app in &mut apps {
        app.hidden = manual.iter().any(|m| m.id == app.id && m.hidden);
    }
    let visible: Vec<_> = apps.iter().filter(|a| !a.hidden).cloned().collect();
    state.set_apps_for_server(&server_id, visible.clone());
    state.event_hub.push_apps(&server_id, visible);
    Ok(apps)
}

#[tauri::command]
pub fn save_onboarding_app_selection(
    state: State<AppState>,
    server_id: String,
    app_ids: Vec<String>,
    selected_ids: Vec<String>,
) -> Result<(), String> {
    state
        .config
        .lock()
        .get_server(&server_id)
        .map_err(|e| e.to_string())?;
    if app_ids.len() > 5000 {
        return Err("Choose at most 5000 applications at a time".into());
    }
    {
        let store = state.metrics.lock();
        let transaction = store
            .connection()
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        for id in &app_ids {
            crate::apps::set_app_hidden(&transaction, &server_id, id, !selected_ids.contains(id))
                .map_err(|e| e.to_string())?;
        }
        transaction.commit().map_err(|e| e.to_string())?;
    }
    for id in &app_ids {
        state.set_app_hidden_cache(&server_id, id, !selected_ids.contains(id));
    }
    state.request_app_rediscovery(&server_id);
    state
        .event_hub
        .push_apps(&server_id, state.apps_for_server(&server_id));
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetSummary {
    server_id: String,
    app_count: usize,
    active_alert_count: i64,
    last_seen_ms: Option<i64>,
    cpu_percent: Option<f64>,
    memory_percent: Option<f64>,
    disk_percent: Option<f64>,
}

#[tauri::command]
pub fn get_fleet_summary(state: State<AppState>) -> Result<Vec<FleetSummary>, String> {
    let servers = state.config.lock().list_servers().to_vec();
    let store = state.metrics.lock();
    let conn = store.connection();
    servers.iter().map(|server| {
        let latest = |key: &str| -> rusqlite::Result<Option<f64>> {
            use rusqlite::OptionalExtension;
            conn.query_row("SELECT value FROM metric_raw WHERE server_id = ?1 AND metric_key = ?2 ORDER BY ts_ms DESC LIMIT 1", rusqlite::params![server.id, key], |r| r.get(0)).optional()
        };
        let last_seen_ms = conn.query_row("SELECT MAX(seen) FROM (SELECT MAX(ts_ms) AS seen FROM metric_raw WHERE server_id = ?1 UNION ALL SELECT MAX(bucket_ts_ms) AS seen FROM metric_rollup WHERE server_id = ?1)", [&server.id], |r| r.get(0)).map_err(|e| e.to_string())?;
        let active_alert_count = conn.query_row("SELECT COUNT(*) FROM alert_events WHERE server_id = ?1 AND resolved_at_ms IS NULL", [&server.id], |r| r.get(0)).map_err(|e| e.to_string())?;
        let cached = state.apps_for_server(&server.id);
        let app_count = if cached.is_empty() { crate::apps::load_manual_apps(conn, &server.id).map_err(|e| e.to_string())?.iter().filter(|a| !a.hidden).count() } else { cached.iter().filter(|a| !a.hidden).count() };
        Ok(FleetSummary { server_id: server.id.clone(), app_count, active_alert_count, last_seen_ms, cpu_percent: latest("cpu.percent").map_err(|e| e.to_string())?, memory_percent: latest("memory.percent").map_err(|e| e.to_string())?, disk_percent: latest("disk.percent").map_err(|e| e.to_string())? })
    }).collect()
}

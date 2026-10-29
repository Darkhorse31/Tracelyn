pub mod apps;
pub mod fleet;
pub mod diagnostics;
pub mod setup;
pub mod ops;
pub mod insight;
pub mod intel;
pub mod logs;
pub mod redis;
pub mod storage_alerts;
pub mod webtraffic;

use crate::config::{AuthType, ConfigError, ServerRecord};
use crate::secrets::{delete_secrets, set_secret, SecretKind};
use crate::ssh::{test_connection, ConnectionTestResult};
use crate::state::{AppState, ConnectionStatus, ServerRuntime, ServerStatus};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertServerInput {
    pub id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub key_path: Option<String>,
    pub labels: Vec<String>,
    pub password: Option<String>,
    pub key_passphrase: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionInput {
    pub server_id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub key_path: Option<String>,
    pub password: Option<String>,
    pub key_passphrase: Option<String>,
}

fn map_err(err: impl ToString) -> String {
    err.to_string()
}

#[tauri::command]
pub fn list_servers(state: State<AppState>) -> Result<Vec<ServerRecord>, String> {
    let config = state.config.lock();
    Ok(config.list_servers().to_vec())
}

#[tauri::command]
pub fn list_server_statuses(state: State<AppState>) -> Vec<ServerStatus> {
    state.all_statuses()
}

#[tauri::command]
pub fn upsert_server(
    state: State<AppState>,
    input: UpsertServerInput,
) -> Result<ServerRecord, String> {
    let mut config = state.config.lock();
    let record = if let Some(id) = input.id {
        let record = ServerRecord {
            id,
            name: input.name,
            host: input.host,
            port: input.port,
            username: input.username,
            auth_type: input.auth_type,
            key_path: input.key_path,
            labels: input.labels,
        };
        config.upsert_server(record)
    } else {
        config.create_server(
            input.name,
            input.host,
            input.port,
            input.username,
            input.auth_type,
            input.key_path,
            input.labels,
        )
    };

    if let Some(password) = input.password.filter(|p| !p.is_empty()) {
        set_secret(&record.id, SecretKind::Password, &password).map_err(map_err)?;
    }
    if let Some(passphrase) = input.key_passphrase.filter(|p| !p.is_empty()) {
        set_secret(&record.id, SecretKind::KeyPassphrase, &passphrase).map_err(map_err)?;
    }

    config.save().map_err(map_err)?;
    Ok(record)
}

#[tauri::command]
pub async fn delete_server(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.connections.disconnect(&id).await;
    state.ops.drop_server(&id).await;
    crate::infra::remove_server_data(&state, &id)?;
    let mut config = state.config.lock();
    config.delete_server(&id).map_err(map_err)?;
    config.save().map_err(map_err)?;
    delete_secrets(&id);
    drop(config);
    state.set_runtime(
        &id,
        ServerRuntime {
            status: ConnectionStatus::Disconnected,
            os_info: None,
            hostname: None,
            last_error: None,
        },
    );
    let _ = app.emit("server-statuses", &state.all_statuses());
    Ok(())
}

#[tauri::command]
pub async fn trust_host_key(
    state: State<'_, AppState>,
    host: String,
    port: u16,
    fingerprint: String,
) -> Result<(), String> {
    let host = host.trim().to_string();
    if host.is_empty() || host.len() > 253 || port == 0 {
        return Err("Host and port are required".into());
    }
    let mut store = state.host_keys.lock().await;
    store.trust(&host, port, &fingerprint).map_err(map_err)
}

#[tauri::command]
pub fn read_crash_log() -> String {
    crate::crash::read_crash_log()
}

#[tauri::command]
pub fn crash_log_path() -> String {
    crate::crash::crash_log_path().display().to_string()
}

#[tauri::command]
pub fn open_crash_log(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let path = crate::crash::crash_log_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(map_err)?;
    }
    if !path.exists() {
        std::fs::write(&path, "").map_err(map_err)?;
    }
    app.opener()
        .open_path(path.to_string_lossy().to_string(), None::<&str>)
        .map_err(map_err)
}

#[tauri::command]
pub async fn test_server_connection(
    state: State<'_, AppState>,
    input: TestConnectionInput,
) -> Result<ConnectionTestResult, String> {
    let server = resolve_server_input(&state, &input)?;
    let password_override = input.password.filter(|p| !p.is_empty());
    let key_passphrase_override = input.key_passphrase.filter(|p| !p.is_empty());
    test_connection(
        &server,
        password_override,
        key_passphrase_override,
        state.host_keys.clone(),
    )
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn connect_server(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<ServerStatus, String> {
    let server = {
        let config = state.config.lock();
        config.get_server(&id).map_err(map_err)?.clone()
    };

    state
        .connections
        .connect(app.clone(), server)
        .await
        .map_err(map_err)?;

    Ok(state
        .all_statuses()
        .into_iter()
        .find(|s| s.id == id)
        .expect("server status"))
}

#[tauri::command]
pub async fn disconnect_server(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<ServerStatus, String> {
    state.connections.disconnect(&id).await;
    state.ops.drop_server(&id).await;
    state.set_runtime(
        &id,
        ServerRuntime {
            status: ConnectionStatus::Disconnected,
            os_info: None,
            hostname: None,
            last_error: None,
        },
    );
    let _ = app.emit("server-statuses", &state.all_statuses());
    Ok(state
        .all_statuses()
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| ConfigError::NotFound(id).to_string())?)
}

#[tauri::command]
pub fn subscribe_overview_metrics(state: State<AppState>) {
    state.event_hub.subscribe_overview();
}

#[tauri::command]
pub fn unsubscribe_overview_metrics(state: State<AppState>) {
    state.event_hub.unsubscribe_overview();
}

#[tauri::command]
pub fn subscribe_server_metrics(state: State<AppState>, server_id: String) {
    state.event_hub.subscribe_server_detail(&server_id);
}

#[tauri::command]
pub fn unsubscribe_server_metrics(state: State<AppState>, server_id: String) {
    state.event_hub.unsubscribe_server_detail(&server_id);
}

#[tauri::command]
pub fn set_live_metrics(state: State<AppState>, server_id: String, enabled: bool) {
    state.set_live_metrics(&server_id, enabled);
}

#[tauri::command]
pub fn subscribe_processes(state: State<AppState>, server_id: String) {
    state.event_hub.subscribe_processes(&server_id);
}

#[tauri::command]
pub fn unsubscribe_processes(state: State<AppState>, server_id: String) {
    state.event_hub.unsubscribe_processes(&server_id);
}

#[tauri::command]
pub fn subscribe_web_traffic(state: State<AppState>, server_id: String) {
    state.event_hub.subscribe_web_traffic(&server_id);
}

#[tauri::command]
pub fn unsubscribe_web_traffic(state: State<AppState>, server_id: String) {
    state.event_hub.unsubscribe_web_traffic(&server_id);
}

#[tauri::command]
pub fn subscribe_apm_metrics(state: State<AppState>, server_id: String) {
    state.event_hub.subscribe_apm(&server_id);
}

#[tauri::command]
pub fn unsubscribe_apm_metrics(state: State<AppState>, server_id: String) {
    state.event_hub.unsubscribe_apm(&server_id);
}

#[tauri::command]
pub fn subscribe_redis_metrics(state: State<AppState>, server_id: String) {
    state.event_hub.subscribe_redis(&server_id);
}

#[tauri::command]
pub fn unsubscribe_redis_metrics(state: State<AppState>, server_id: String) {
    state.event_hub.unsubscribe_redis(&server_id);
}

fn resolve_server_input(
    state: &AppState,
    input: &TestConnectionInput,
) -> Result<ServerRecord, String> {
    if let Some(id) = &input.server_id {
        let config = state.config.lock();
        return config.get_server(id).map_err(map_err).cloned();
    }
    Ok(ServerRecord {
        id: "ephemeral".into(),
        name: input.name.clone(),
        host: input.host.clone(),
        port: input.port,
        username: input.username.clone(),
        auth_type: input.auth_type.clone(),
        key_path: input.key_path.clone(),
        labels: vec![],
    })
}

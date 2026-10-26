use crate::collectors::redis::{ping, Dial};
use crate::config::RedisTargetConfig;
use crate::connection::redis_probe::probe_redis_endpoints;
use crate::secrets::{get_secret, set_secret, SecretError, SecretKind};
use crate::ssh::connect_persistent;
use crate::state::AppState;
use russh::Disconnect;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisAuthStatus {
    pub configured: bool,
}

fn map_err(err: impl ToString) -> String {
    err.to_string()
}

#[tauri::command]
pub fn set_redis_password(server_id: String, password: String) -> Result<(), String> {
    if password.is_empty() {
        return Err("password cannot be empty".into());
    }
    set_secret(&server_id, SecretKind::RedisPassword, &password).map_err(map_err)
}

#[tauri::command]
pub fn redis_auth_status(server_id: String) -> Result<RedisAuthStatus, String> {
    let configured = match get_secret(&server_id, SecretKind::RedisPassword) {
        Ok(_) => true,
        Err(SecretError::NotFound(_)) => false,
        Err(err) => return Err(map_err(err)),
    };
    Ok(RedisAuthStatus { configured })
}

#[tauri::command]
pub fn set_redis_username(server_id: String, username: String) -> Result<(), String> {
    if username.is_empty() {
        return Ok(());
    }
    set_secret(&server_id, SecretKind::RedisUsername, &username).map_err(map_err)
}

#[tauri::command]
pub fn set_redis_target(
    state: State<AppState>,
    server_id: String,
    target: RedisTargetConfig,
) -> Result<(), String> {
    let mut config = state.config.lock();
    config
        .set_redis_target(&server_id, target)
        .map_err(map_err)?;
    config.save().map_err(map_err)
}

#[tauri::command]
pub fn get_redis_target(state: State<AppState>, server_id: String) -> RedisTargetConfig {
    state.config.lock().redis_target(&server_id)
}

#[tauri::command]
pub async fn test_redis_connection(
    app: AppHandle,
    server_id: String,
    target: Option<RedisTargetConfig>,
) -> Result<String, String> {
    let state = app.state::<AppState>();
    let server = state
        .config
        .lock()
        .get_server(&server_id)
        .map_err(map_err)?
        .clone();
    let saved = target.unwrap_or_else(|| state.config.lock().redis_target(&server_id));
    let host_keys = state.host_keys.clone();
    let session = connect_persistent(&server, host_keys)
        .await
        .map_err(map_err)?;
    let endpoints = probe_redis_endpoints(&session).await;
    let mut dial = endpoints
        .first()
        .map(|endpoint| endpoint.to_dial())
        .unwrap_or_else(|| Dial::loopback(saved.port.unwrap_or(6379)));
    if let Some(port) = saved.port {
        dial.port = port;
        dial.host = "127.0.0.1".into();
        dial.bind_label = format!("127.0.0.1:{port}");
    }
    if let Some(socket) = saved.socket.filter(|value| !value.is_empty()) {
        dial.socket = Some(socket);
    }
    if let Some(container) = saved.docker_container.filter(|value| !value.is_empty()) {
        dial.docker = Some(container);
    }
    let password = match get_secret(&server_id, SecretKind::RedisPassword) {
        Ok(value) => Some(value),
        Err(SecretError::NotFound(_)) => None,
        Err(err) => return Err(map_err(err)),
    };
    let username = get_secret(&server_id, SecretKind::RedisUsername).ok();
    let result = ping(
        &session,
        &dial,
        username.as_deref(),
        password.as_deref(),
    )
    .await
    .map_err(|err| err.message());
    let _ = session.disconnect(Disconnect::ByApplication, "", "en").await;
    result
}

#[tauri::command]
pub fn get_redis_snapshot(
    state: State<AppState>,
    server_id: String,
) -> Option<serde_json::Value> {
    state.event_hub.last_redis_snapshot(&server_id)
}

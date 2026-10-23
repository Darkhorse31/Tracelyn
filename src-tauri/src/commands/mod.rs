pub mod webtraffic;

use crate::config::{AuthType, ConfigError, ServerRecord};
use crate::secrets::{delete_secrets, set_secret, SecretKind};
use crate::ssh::{connect_and_probe, test_connection, ConnectionTestResult};
use crate::state::{AppState, ConnectionStatus, ServerRuntime, ServerStatus};
use serde::Deserialize;
use tauri::State;

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
pub fn delete_server(state: State<AppState>, id: String) -> Result<(), String> {
    let mut config = state.config.lock();
    config.delete_server(&id).map_err(map_err)?;
    config.save().map_err(map_err)?;
    delete_secrets(&id);
    state.set_runtime(
        &id,
        ServerRuntime {
            status: ConnectionStatus::Disconnected,
            os_info: None,
            hostname: None,
            last_error: None,
        },
    );
    Ok(())
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
pub async fn connect_server(state: State<'_, AppState>, id: String) -> Result<ServerStatus, String> {
    let server = {
        let config = state.config.lock();
        config.get_server(&id).map_err(map_err)?.clone()
    };

    state.set_runtime(
        &id,
        ServerRuntime {
            status: ConnectionStatus::Connecting,
            os_info: None,
            hostname: None,
            last_error: None,
        },
    );

    match connect_and_probe(&server, state.host_keys.clone()).await {
        Ok(result) => {
            let runtime = ServerRuntime {
                status: ConnectionStatus::Connected,
                os_info: Some(result.os_info),
                hostname: result.hostname,
                last_error: None,
            };
            state.set_runtime(&id, runtime);
            Ok(state
                .all_statuses()
                .into_iter()
                .find(|s| s.id == id)
                .expect("server status"))
        }
        Err(err) => {
            let runtime = ServerRuntime {
                status: ConnectionStatus::Offline,
                os_info: None,
                hostname: None,
                last_error: Some(err.to_string()),
            };
            state.set_runtime(&id, runtime);
            Err(err.to_string())
        }
    }
}

#[tauri::command]
pub fn disconnect_server(state: State<AppState>, id: String) -> Result<ServerStatus, String> {
    state.set_runtime(
        &id,
        ServerRuntime {
            status: ConnectionStatus::Disconnected,
            os_info: None,
            hostname: None,
            last_error: None,
        },
    );
    Ok(state
        .all_statuses()
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| ConfigError::NotFound(id).to_string())?)
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

use crate::collectors::redis::RedisSnapshot;
use crate::secrets::{get_secret, set_secret, SecretError, SecretKind};
use serde::Serialize;

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

/// Parses fixture files and returns a snapshot (for UI/dev validation without SSH).
#[tauri::command]
pub fn redis_snapshot_demo() -> Result<RedisSnapshot, String> {
    use crate::collectors::redis::{
        derive_metrics, parse_config_get_maxmemory, parse_info_all, parse_info_keyspace,
        parse_slowlog_get, RateState, RedisSnapshot,
    };
    use std::time::Duration;

    fn fixture(name: &str) -> Result<String, String> {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/redis/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        ))
        .map_err(|e| e.to_string())
    }

    let info = parse_info_all(&fixture("info_all.txt")?).map_err(map_err)?;
    let keyspace = parse_info_keyspace(&fixture("info_keyspace.txt")?).map_err(map_err)?;
    let slowlog = parse_slowlog_get(&fixture("slowlog_get.txt")?).map_err(map_err)?;
    let maxmemory = parse_config_get_maxmemory(&fixture("config_maxmemory.txt")?).map_err(map_err)?;

    let mut rate = RateState::default();
    let derived = derive_metrics(
        &info,
        &keyspace,
        maxmemory,
        &mut rate,
        Duration::from_secs(5),
    );

    Ok(RedisSnapshot::from_parts(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        info,
        keyspace,
        slowlog,
        derived,
    ))
}

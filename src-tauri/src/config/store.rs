use crate::webserver::RouteRule;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("server not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    Key,
    Password,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerRecord {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub servers: Vec<ServerRecord>,
    /// Manual APM route-normalization rules keyed by server id.
    #[serde(default)]
    pub route_rules: HashMap<String, Vec<RouteRule>>,
    /// Optional Redis endpoint override (port, socket, or docker container).
    #[serde(default)]
    pub redis_targets: HashMap<String, RedisTargetConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RedisTargetConfig {
    pub port: Option<u16>,
    pub socket: Option<String>,
    pub docker_container: Option<String>,
    pub label: Option<String>,
}

pub struct ConfigStore {
    path: PathBuf,
    config: AppConfig,
}

impl ConfigStore {
    pub fn load() -> Result<Self, ConfigError> {
        let path = config_path();
        let config = if path.exists() {
            let raw = fs::read_to_string(&path)?;
            serde_json::from_str(&raw)?
        } else {
            AppConfig::default()
        };
        Ok(Self { path, config })
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(&self.config)?;
        fs::write(&self.path, raw)?;
        Ok(())
    }

    pub fn list_servers(&self) -> &[ServerRecord] {
        &self.config.servers
    }

    /// Imports save as one operation. A failed write leaves the active config intact.
    pub fn import_servers(&mut self, records: Vec<ServerRecord>) -> Result<(), ConfigError> {
        let mut next = self.config.clone();
        next.servers.extend(records);
        if let Some(parent) = self.path.parent() { fs::create_dir_all(parent)?; }
        let temporary = self.path.with_extension("import.tmp");
        fs::write(&temporary, serde_json::to_string_pretty(&next)?)?;
        if let Err(error) = fs::rename(&temporary, &self.path) {
            let _ = fs::remove_file(&temporary);
            return Err(ConfigError::Io(error));
        }
        self.config = next;
        Ok(())
    }

    pub fn get_server(&self, id: &str) -> Result<&ServerRecord, ConfigError> {
        self.config
            .servers
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| ConfigError::NotFound(id.to_string()))
    }

    pub fn upsert_server(&mut self, record: ServerRecord) -> ServerRecord {
        if let Some(existing) = self.config.servers.iter_mut().find(|s| s.id == record.id) {
            *existing = record.clone();
        } else {
            self.config.servers.push(record.clone());
        }
        record
    }

    pub fn create_server(
        &mut self,
        name: String,
        host: String,
        port: u16,
        username: String,
        auth_type: AuthType,
        key_path: Option<String>,
        labels: Vec<String>,
    ) -> ServerRecord {
        let record = ServerRecord {
            id: Uuid::new_v4().to_string(),
            name,
            host,
            port,
            username,
            auth_type,
            key_path,
            labels,
        };
        self.config.servers.push(record.clone());
        record
    }

    pub fn delete_server(&mut self, id: &str) -> Result<ServerRecord, ConfigError> {
        let pos = self
            .config
            .servers
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| ConfigError::NotFound(id.to_string()))?;
        self.config.route_rules.remove(id);
        self.config.redis_targets.remove(id);
        Ok(self.config.servers.remove(pos))
    }

    pub fn route_rules(&self, server_id: &str) -> Vec<RouteRule> {
        self.config
            .route_rules
            .get(server_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_route_rules(
        &mut self,
        server_id: &str,
        rules: Vec<RouteRule>,
    ) -> Result<(), ConfigError> {
        self.get_server(server_id)?;
        if rules.is_empty() {
            self.config.route_rules.remove(server_id);
        } else {
            self.config.route_rules.insert(server_id.to_string(), rules);
        }
        Ok(())
    }

    pub fn redis_target(&self, server_id: &str) -> RedisTargetConfig {
        self.config
            .redis_targets
            .get(server_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_redis_target(
        &mut self,
        server_id: &str,
        target: RedisTargetConfig,
    ) -> Result<(), ConfigError> {
        self.get_server(server_id)?;
        let empty = target.port.is_none()
            && target.socket.as_ref().map(|s| s.is_empty()).unwrap_or(true)
            && target
                .docker_container
                .as_ref()
                .map(|s| s.is_empty())
                .unwrap_or(true)
            && target.label.as_ref().map(|s| s.is_empty()).unwrap_or(true);
        if empty {
            self.config.redis_targets.remove(server_id);
        } else {
            self.config
                .redis_targets
                .insert(server_id.to_string(), target);
        }
        Ok(())
    }
}

fn config_path() -> PathBuf {
    let base = dirs::data_local_dir().expect("local data dir");
    base.join("Tracelyn").join("config.json")
}

#[cfg(test)]
mod import_tests {
    use super::*;

    fn profile(name: &str) -> ServerRecord {
        ServerRecord { id: name.into(), name: name.into(), host: format!("{name}.example.com"), port: 22, username: "deploy".into(), auth_type: AuthType::Key, key_path: None, labels: vec!["production".into()] }
    }

    #[test]
    fn bulk_import_preserves_existing_profiles_and_replaces_file() {
        let directory = std::env::temp_dir().join(format!("tracelyn-import-{}", Uuid::new_v4()));
        let path = directory.join("config.json");
        let mut store = ConfigStore { path: path.clone(), config: AppConfig { servers: vec![profile("existing")], ..Default::default() } };
        store.save().unwrap();
        store.import_servers(vec![profile("first"), profile("second")]).unwrap();
        let saved: AppConfig = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved.servers.len(), 3);
        assert_eq!(saved.servers[0].name, "existing");
        assert_eq!(store.list_servers().len(), 3);
        assert!(!path.with_extension("import.tmp").exists());
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn bulk_import_keeps_memory_unchanged_when_save_fails() {
        let blocker = std::env::temp_dir().join(format!("tracelyn-import-blocker-{}", Uuid::new_v4()));
        fs::write(&blocker, "not a directory").unwrap();
        let mut store = ConfigStore { path: blocker.join("config.json"), config: AppConfig { servers: vec![profile("existing")], ..Default::default() } };
        assert!(store.import_servers(vec![profile("new")]).is_err());
        assert_eq!(store.list_servers().len(), 1);
        assert_eq!(fs::read_to_string(&blocker).unwrap(), "not a directory");
        fs::remove_file(blocker).unwrap();
    }
}

use serde::{Deserialize, Serialize};
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
        Ok(self.config.servers.remove(pos))
    }
}

fn config_path() -> PathBuf {
    let base = dirs::data_local_dir().expect("local data dir");
    base.join("ServerLens").join("config.json")
}

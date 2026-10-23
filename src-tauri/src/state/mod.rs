use crate::config::ConfigStore;
use crate::connection::ConnectionManager;
use crate::events::EventHub;
use crate::ssh::HostKeyStore;
use crate::storage::{default_db_path, MetricStore};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Degraded,
    Offline,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub id: String,
    pub status: ConnectionStatus,
    pub os_info: Option<String>,
    pub hostname: Option<String>,
    pub last_error: Option<String>,
}

pub struct AppState {
    pub config: Mutex<ConfigStore>,
    pub host_keys: Arc<AsyncMutex<HostKeyStore>>,
    pub event_hub: Arc<EventHub>,
    pub connections: ConnectionManager,
    pub metrics: Mutex<MetricStore>,
    connection_status: Mutex<HashMap<String, ServerRuntime>>,
}

#[derive(Debug, Clone)]
pub struct ServerRuntime {
    pub status: ConnectionStatus,
    pub os_info: Option<String>,
    pub hostname: Option<String>,
    pub last_error: Option<String>,
}

impl Default for ServerRuntime {
    fn default() -> Self {
        Self {
            status: ConnectionStatus::Disconnected,
            os_info: None,
            hostname: None,
            last_error: None,
        }
    }
}

impl AppState {
    pub fn new(
        config: ConfigStore,
        host_keys: Arc<AsyncMutex<HostKeyStore>>,
        event_hub: Arc<EventHub>,
        connections: ConnectionManager,
    ) -> Self {
        let metrics = MetricStore::open(default_db_path()).expect("failed to open metrics store");
        Self {
            config: Mutex::new(config),
            host_keys,
            event_hub,
            connections,
            metrics: Mutex::new(metrics),
            connection_status: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_runtime(&self, id: &str, runtime: ServerRuntime) {
        self.connection_status
            .lock()
            .insert(id.to_string(), runtime);
    }

    pub fn runtime(&self, id: &str) -> ServerRuntime {
        self.connection_status
            .lock()
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn all_statuses(&self) -> Vec<ServerStatus> {
        let config = self.config.lock();
        config
            .list_servers()
            .iter()
            .map(|s| {
                let rt = self.runtime(&s.id);
                ServerStatus {
                    id: s.id.clone(),
                    status: rt.status,
                    os_info: rt.os_info,
                    hostname: rt.hostname,
                    last_error: rt.last_error,
                }
            })
            .collect()
    }
}

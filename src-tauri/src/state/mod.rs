use crate::apps::TracelynApp;
use crate::config::ConfigStore;
use crate::connection::ConnectionManager;
use crate::diagnostics::DiagnosticEntry;
use crate::events::{EventHub, LogStreamSnapshot};
use crate::ssh::{HostKeyStore, SshSession};
use crate::storage::{default_db_path, MetricStore};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
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
    pub ops: crate::ops::OpsRuntime,
    connection_status: Mutex<HashMap<String, ServerRuntime>>,
    apps_cache: Mutex<HashMap<String, Vec<TracelynApp>>>,
    diagnostics_cache: Mutex<HashMap<String, Vec<DiagnosticEntry>>>,
    rediscovery_requested: Mutex<HashSet<String>>,
    log_paused: Mutex<HashSet<String>>,
    log_filters: Mutex<HashMap<String, LogFilter>>,
    log_windows: Mutex<HashMap<String, LogStreamSnapshot>>,
    sessions: Mutex<HashMap<String, Arc<SshSession>>>,
    live_metrics: Mutex<HashSet<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    pub generation: u64,
    pub app_ids: Vec<String>,
    pub sources: Vec<String>,
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
        if let Err(e) = crate::apps::migrate_app_registry(metrics.connection()) {
            eprintln!("app_registry migrate: {}", e);
        }
        Self {
            config: Mutex::new(config),
            host_keys,
            event_hub,
            connections,
            metrics: Mutex::new(metrics),
            ops: crate::ops::OpsRuntime::new(),
            connection_status: Mutex::new(HashMap::new()),
            apps_cache: Mutex::new(HashMap::new()),
            diagnostics_cache: Mutex::new(HashMap::new()),
            rediscovery_requested: Mutex::new(HashSet::new()),
            log_paused: Mutex::new(HashSet::new()),
            log_filters: Mutex::new(HashMap::new()),
            log_windows: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            live_metrics: Mutex::new(HashSet::new()),
        }
    }

    pub fn attach_session(&self, server_id: &str, session: Arc<SshSession>) {
        self.sessions.lock().insert(server_id.to_string(), session);
    }

    pub fn detach_session(&self, server_id: &str) {
        self.sessions.lock().remove(server_id);
    }

    pub fn session(&self, server_id: &str) -> Option<Arc<SshSession>> {
        self.sessions.lock().get(server_id).cloned()
    }

    pub fn set_live_metrics(&self, server_id: &str, enabled: bool) {
        let mut set = self.live_metrics.lock();
        if enabled {
            set.insert(server_id.to_string());
        } else {
            set.remove(server_id);
        }
    }

    pub fn live_metrics(&self, server_id: &str) -> bool {
        self.live_metrics.lock().contains(server_id)
    }

    pub fn set_log_window(&self, server_id: &str, snapshot: LogStreamSnapshot) {
        self.log_windows
            .lock()
            .insert(server_id.to_string(), snapshot);
    }

    pub fn log_window(&self, server_id: &str) -> Option<LogStreamSnapshot> {
        self.log_windows.lock().get(server_id).cloned()
    }

    pub fn set_log_paused(&self, server_id: &str, paused: bool) {
        let mut set = self.log_paused.lock();
        if paused {
            set.insert(server_id.to_string());
        } else {
            set.remove(server_id);
        }
    }

    pub fn log_paused(&self, server_id: &str) -> bool {
        self.log_paused.lock().contains(server_id)
    }

    pub fn set_log_filter(&self, server_id: &str, app_ids: Vec<String>, sources: Vec<String>) -> u64 {
        let mut map = self.log_filters.lock();
        let entry = map.entry(server_id.to_string()).or_default();
        entry.generation = entry.generation.wrapping_add(1);
        entry.app_ids = app_ids;
        entry.sources = sources;
        entry.generation
    }

    pub fn log_filter(&self, server_id: &str) -> LogFilter {
        self.log_filters
            .lock()
            .get(server_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn apps_for_server(&self, server_id: &str) -> Vec<TracelynApp> {
        self.apps_cache
            .lock()
            .get(server_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_apps_for_server(&self, server_id: &str, apps: Vec<TracelynApp>) {
        self.apps_cache.lock().insert(server_id.to_string(), apps);
    }

    pub fn upsert_cached_app(&self, app: &TracelynApp) {
        let mut cache = self.apps_cache.lock();
        let list = cache.entry(app.server_id.clone()).or_default();
        if let Some(idx) = list.iter().position(|a| a.id == app.id) {
            list[idx] = app.clone();
        } else {
            list.push(app.clone());
        }
    }

    pub fn set_app_hidden_cache(&self, server_id: &str, app_id: &str, hidden: bool) {
        let mut cache = self.apps_cache.lock();
        if let Some(list) = cache.get_mut(server_id) {
            if hidden {
                list.retain(|a| a.id != app_id);
            }
        }
    }

    pub fn diagnostics_for_server(&self, server_id: &str) -> Vec<DiagnosticEntry> {
        self.diagnostics_cache
            .lock()
            .get(server_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_diagnostics(&self, server_id: &str, entries: Vec<DiagnosticEntry>) {
        self.diagnostics_cache
            .lock()
            .insert(server_id.to_string(), entries);
    }

    pub fn request_app_rediscovery(&self, server_id: &str) {
        self.rediscovery_requested.lock().insert(server_id.to_string());
    }

    pub fn take_rediscovery_requested(&self, server_id: &str) -> bool {
        self.rediscovery_requested.lock().remove(server_id)
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

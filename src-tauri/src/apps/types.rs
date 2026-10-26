use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppKind {
    Pm2,
    Systemd,
    Docker,
    Nginx,
    Apache,
    Service,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppOrigin {
    Discovered,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppStatus {
    Online,
    Stopped,
    Errored,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogSourceKind {
    Stdout,
    Stderr,
    Journald,
    Docker,
    NginxAccess,
    NginxError,
    ApacheAccess,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSourceRef {
    pub id: String,
    pub kind: LogSourceKind,
    pub path: Option<String>,
    pub container_id: Option<String>,
    pub vhost: Option<String>,
    pub readable: bool,
    pub issue: Option<String>,
    pub fix_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMetrics {
    pub req_per_sec: Option<f64>,
    pub error_rate: Option<f64>,
    pub p95_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TracelynApp {
    pub id: String,
    pub server_id: String,
    pub name: String,
    pub kind: AppKind,
    pub status: AppStatus,
    pub origin: AppOrigin,
    pub hidden: bool,
    pub pids: Vec<u32>,
    pub ports: Vec<u16>,
    pub domains: Vec<String>,
    pub tags: Vec<String>,
    pub log_sources: Vec<LogSourceRef>,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub restarts: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub cwd: Option<String>,
    pub interpreter: Option<String>,
    pub linked_app_ids: Vec<String>,
    pub metrics: Option<AppMetrics>,
}

impl TracelynApp {
    pub fn discovered_id(server_id: &str, kind: AppKind, name: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        server_id.hash(&mut h);
        kind.hash(&mut h);
        name.hash(&mut h);
        format!("app-{:x}", h.finish())
    }
}

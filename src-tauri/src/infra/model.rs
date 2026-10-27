use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Loading,
    Ok,
    Partial,
    NotInstalled,
    AuthNeeded,
    PermissionDenied,
    TimedOut,
    Error,
    InstalledStopped,
    NotConfigured,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub id: String,
    pub name: String,
    pub state: String,
    pub fields: Map<String, Value>,
}
impl Resource {
    pub fn new(name: impl Into<String>, state: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            id: name.clone(),
            name,
            state: state.into(),
            fields: Map::new(),
        }
    }
    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        self.fields.insert(key.into(), value.into());
    }
    pub fn number(&self, key: &str) -> Option<f64> {
        self.fields
            .get(key)
            .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub collector: String,
    pub phase: Phase,
    pub message: String,
    pub fix_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub sampled_at_ms: i64,
    pub rows: Vec<Resource>,
}
impl Snapshot {
    pub fn ok(collector: &str, rows: Vec<Resource>) -> Self {
        Self {
            collector: collector.into(),
            phase: Phase::Ok,
            message: format!("{} resources", rows.len()),
            fix_hint: None,
            detail: None,
            sampled_at_ms: crate::metrics_ingest::now_ms(),
            rows,
        }
    }
    pub fn problem(collector: &str, phase: Phase, message: &str, hint: &str) -> Self {
        Self {
            collector: collector.into(),
            phase,
            message: message.into(),
            fix_hint: Some(hint.into()),
            detail: None,
            sampled_at_ms: crate::metrics_ingest::now_ms(),
            rows: vec![],
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Target {
    pub host: String,
    pub port: Option<u16>,
    pub container: String,
    pub database: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UptimeCheck {
    pub id: String,
    pub server_id: String,
    pub url: String,
    pub interval_seconds: u64,
    pub enabled: bool,
    /// HTTPS checks verify the certificate unless this is set.
    #[serde(default)]
    pub allow_self_signed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub ts_ms: i64,
    pub ok: bool,
    pub status_code: Option<u16>,
    pub addresses: Vec<String>,
    pub dns_ms: Option<f64>,
    pub connect_ms: Option<f64>,
    pub tls_ms: Option<f64>,
    pub ttfb_ms: Option<f64>,
    pub total_ms: f64,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckView {
    pub check: UptimeCheck,
    pub history: Vec<CheckResult>,
}

pub const COLLECTORS: &[&str] = &[
    "mysql",
    "postgres",
    "mongo",
    "rabbitmq",
    "kafka",
    "haproxy",
    "nginx_upstreams",
    "traefik",
    "caddy",
    "docker_infra",
    "kubernetes",
    "certificates",
    "certificates_pc",
    "uptime",
    "cron",
    "capacity",
    "ssh_security",
    "ports",
    "updates",
    "fail2ban",
    "last_logins",
];

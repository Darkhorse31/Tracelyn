use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IntelError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0}")]
    Bad(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleParams {
    pub threshold: f64,
    #[serde(default)]
    pub clear_threshold: Option<f64>,
    #[serde(default)]
    pub duration_secs: u64,
    /// How long the value must stay in the clear band before the alert resolves.
    #[serde(default = "default_clear_secs")]
    pub clear_duration_secs: u64,
    #[serde(default)]
    pub metric_key: String,
    #[serde(default)]
    pub pattern: String,
    #[serde(default = "default_window")]
    pub window_secs: u64,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    /// Suppress a repeat notification for this long after the last one.
    #[serde(default)]
    pub cooldown_secs: u64,
    /// Local minute of day when quiet hours start. Together with `quiet_end_min`.
    #[serde(default)]
    pub quiet_start_min: Option<u16>,
    #[serde(default)]
    pub quiet_end_min: Option<u16>,
    /// Empty means every app on the server.
    #[serde(default)]
    pub app_id: String,
}

fn default_clear_secs() -> u64 {
    30
}
fn default_window() -> u64 {
    60
}

impl Default for RuleParams {
    fn default() -> Self {
        Self {
            threshold: 0.0,
            clear_threshold: None,
            duration_secs: 0,
            clear_duration_secs: 30,
            metric_key: String::new(),
            pattern: String::new(),
            window_secs: 60,
            host: String::new(),
            port: 0,
            cooldown_secs: 0,
            quiet_start_min: None,
            quiet_end_min: None,
            app_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntelRule {
    pub id: String,
    #[serde(default)]
    pub server_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub enabled: bool,
    pub severity: String,
    pub params: RuleParams,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// True when any server currently has this rule firing. Not stored on the rule row.
    #[serde(default)]
    pub firing: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RuleRuntime {
    pub firing: bool,
    pub condition_since_ms: Option<i64>,
    pub clear_since_ms: Option<i64>,
    pub last_value: Option<f64>,
    pub last_error: Option<String>,
    pub checked_at_ms: i64,
    pub last_notify_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConfig {
    #[serde(default)]
    pub chat_id: String,
    #[serde(default)]
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    #[serde(default = "default_smtp_tls")]
    pub smtp_tls: String,
    #[serde(default)]
    pub mail_from: String,
    #[serde(default)]
    pub mail_to: String,
    #[serde(default)]
    pub username: String,
}

fn default_smtp_port() -> u16 {
    587
}
fn default_smtp_tls() -> String {
    "starttls".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub config: ChannelConfig,
    #[serde(default)]
    pub has_secret: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Incident {
    pub id: String,
    pub server_id: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub opened_at_ms: i64,
    pub updated_at_ms: i64,
    pub acked_at_ms: Option<i64>,
    pub resolved_at_ms: Option<i64>,
    #[serde(default)]
    pub snooze_until_ms: Option<i64>,
    #[serde(default)]
    pub unread: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEntry {
    pub id: String,
    pub ts_ms: i64,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncidentDetail {
    pub incident: Incident,
    pub timeline: Vec<TimelineEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnomalyEvent {
    pub id: String,
    pub server_id: String,
    pub metric_key: String,
    pub ts_ms: i64,
    pub value: f64,
    pub baseline: f64,
    pub z: f64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BandPoint {
    pub ts_ms: i64,
    pub value: f64,
    pub baseline: f64,
    pub low: f64,
    pub high: f64,
    pub z: f64,
    pub anomalous: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedView {
    pub id: String,
    pub name: String,
    pub path: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DigestSpec {
    pub id: String,
    pub name: String,
    pub cadence: String,
    pub hour_utc: u8,
    pub weekday: u8,
    pub channel_id: String,
    pub enabled: bool,
    pub last_sent_ms: Option<i64>,
    pub last_attempt_ms: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SlotModel {
    pub ewma: f64,
    pub residuals: Vec<f64>,
    pub samples: u32,
}

impl Default for SlotModel {
    fn default() -> Self {
        Self {
            ewma: 0.0,
            residuals: Vec::new(),
            samples: 0,
        }
    }
}

pub const RULE_KINDS: &[&str] = &[
    "metric",
    "log_pattern",
    "error_rate",
    "p95",
    "ssl_expiry",
    "uptime",
    "app_crash",
    "restart_loop",
    "app_down",
    "new_error",
    "disk_forecast",
    "server_unreachable",
    "redis_down",
    "service_stopped",
    "ssh_login",
    "ssh_fail_burst",
    "deploy",
];

pub const CHANNEL_KINDS: &[&str] = &[
    "windows",
    "slack",
    "discord",
    "teams",
    "telegram",
    "email",
    "webhook",
    "ntfy",
];

pub const WATCH_METRICS: &[&str] = &[
    "cpu.percent",
    "memory.percent",
    "disk.percent",
    "load.1",
    "web.req_per_sec",
    "http.5xx_rate_percent",
    "endpoint.p95_ms",
    "app.error_rate",
    "app.p95_ms",
];

pub fn is_low_kind(kind: &str) -> bool {
    matches!(
        kind,
        "ssl_expiry"
            | "uptime"
            | "app_down"
            | "disk_forecast"
            | "server_unreachable"
            | "redis_down"
            | "service_stopped"
    )
}

pub fn default_metric_key(kind: &str, configured: &str) -> String {
    if !configured.is_empty() {
        return configured.to_string();
    }
    match kind {
        "error_rate" => "http.5xx_rate_percent".into(),
        "p95" => "endpoint.p95_ms".into(),
        "metric" => "cpu.percent".into(),
        "disk_forecast" => "disk.forecast_days".into(),
        "server_unreachable" => "server.connected".into(),
        "redis_down" => "redis.up".into(),
        "service_stopped" => "service.nginx".into(),
        "ssh_fail_burst" => "ssh.fail_count".into(),
        "app_down" => "health.up".into(),
        _ => String::new(),
    }
}

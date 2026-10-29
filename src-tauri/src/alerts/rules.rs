use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RuleError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    ServerUnreachable,
    CpuHigh,
    MemoryHigh,
    DiskHigh,
    Http5xxRate,
    EndpointP95,
    RedisMemoryNearMax,
    LogQuery,
    TraceP95,
    TraceErrorRate,
}

impl RuleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ServerUnreachable => "server_unreachable",
            Self::CpuHigh => "cpu_high",
            Self::MemoryHigh => "memory_high",
            Self::DiskHigh => "disk_high",
            Self::Http5xxRate => "http_5xx_rate",
            Self::EndpointP95 => "endpoint_p95",
            Self::RedisMemoryNearMax => "redis_memory_near_max",
            Self::LogQuery => "log_query",
            Self::TraceP95 => "trace_p95",
            Self::TraceErrorRate => "trace_error_rate",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "server_unreachable" => Some(Self::ServerUnreachable),
            "cpu_high" => Some(Self::CpuHigh),
            "memory_high" => Some(Self::MemoryHigh),
            "disk_high" => Some(Self::DiskHigh),
            "http_5xx_rate" => Some(Self::Http5xxRate),
            "endpoint_p95" => Some(Self::EndpointP95),
            "redis_memory_near_max" => Some(Self::RedisMemoryNearMax),
            "log_query" => Some(Self::LogQuery),
            "trace_p95" => Some(Self::TraceP95),
            "trace_error_rate" => Some(Self::TraceErrorRate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleCondition {
    pub metric_key: String,
    #[serde(default = "default_op")]
    pub op: String,
    pub threshold: f64,
}

fn default_op() -> String {
    "gte".into()
}

fn default_window() -> u64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleParams {
    pub threshold: f64,
    #[serde(default)]
    pub clear_threshold: Option<f64>,
    /// Seconds the condition must hold before firing.
    #[serde(default)]
    pub duration_secs: u64,
    /// Optional metric key override (e.g. endpoint route id for p95).
    #[serde(default)]
    pub metric_key: Option<String>,
    #[serde(default)]
    pub endpoint_label: Option<String>,
    /// Every condition must hold. Empty means the rule kind supplies the check.
    #[serde(default)]
    pub conditions: Vec<RuleCondition>,
    #[serde(default)]
    pub cooldown_secs: u64,
    #[serde(default)]
    pub quiet_start_min: Option<u16>,
    #[serde(default)]
    pub quiet_end_min: Option<u16>,
    #[serde(default)]
    pub log_query: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default = "default_window")]
    pub window_secs: u64,
    #[serde(default)]
    pub alert_on_no_data: bool,
    #[serde(default)]
    pub silence_until_ms: Option<i64>,
}

impl Default for RuleParams {
    fn default() -> Self {
        Self {
            threshold: 0.0,
            clear_threshold: None,
            duration_secs: 0,
            metric_key: None,
            endpoint_label: None,
            conditions: Vec::new(),
            cooldown_secs: 0,
            quiet_start_min: None,
            quiet_end_min: None,
            log_query: None,
            endpoint: None,
            window_secs: default_window(),
            alert_on_no_data: false,
            silence_until_ms: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertRule {
    pub id: String,
    pub server_id: Option<String>,
    pub kind: RuleKind,
    pub name: String,
    pub enabled: bool,
    pub params: RuleParams,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// True when any server currently has this rule firing. Not stored on the rule row.
    #[serde(default)]
    pub firing: bool,
    /// True when the latest sample had no metric for this rule.
    #[serde(default)]
    pub no_data: bool,
}

pub fn migrate_rules(conn: &Connection) -> Result<(), RuleError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS alert_rules (
            id TEXT PRIMARY KEY,
            server_id TEXT,
            kind TEXT NOT NULL,
            name TEXT NOT NULL,
            enabled INTEGER NOT NULL,
            params_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS alert_rule_state (
            rule_id TEXT NOT NULL,
            server_id TEXT NOT NULL,
            firing INTEGER NOT NULL,
            condition_since_ms INTEGER,
            clear_since_ms INTEGER,
            last_value REAL,
            no_data INTEGER NOT NULL DEFAULT 0,
            last_notify_ms INTEGER,
            PRIMARY KEY (rule_id, server_id)
        );
        ",
    )?;
    let _ = conn.execute(
        "ALTER TABLE alert_rule_state ADD COLUMN no_data INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE alert_rule_state ADD COLUMN last_notify_ms INTEGER",
        [],
    );
    Ok(())
}

pub fn list_rules(conn: &Connection) -> Result<Vec<AlertRule>, RuleError> {
    let mut stmt = conn.prepare(
        "SELECT id, server_id, kind, name, enabled, params_json, created_at_ms, updated_at_ms
         FROM alert_rules ORDER BY name ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let kind_str: String = row.get(2)?;
        let params_json: String = row.get(5)?;
        Ok(AlertRule {
            id: row.get(0)?,
            server_id: row.get(1)?,
            kind: RuleKind::from_str(&kind_str).unwrap_or(RuleKind::CpuHigh),
            name: row.get(3)?,
            enabled: row.get::<_, i64>(4)? != 0,
            params: serde_json::from_str(&params_json).unwrap_or_else(|_| RuleParams {
                threshold: 90.0,
                ..RuleParams::default()
            }),
            created_at_ms: row.get(6)?,
            updated_at_ms: row.get(7)?,
            firing: false,
            no_data: false,
        })
    })?;
    let mut rules = rows.collect::<Result<Vec<_>, _>>()?;
    attach_status(conn, &mut rules)?;
    Ok(rules)
}

fn attach_status(conn: &Connection, rules: &mut [AlertRule]) -> Result<(), RuleError> {
    let mut stmt = conn.prepare(
        "SELECT rule_id, MAX(firing), MAX(no_data) FROM alert_rule_state GROUP BY rule_id",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let firing: i64 = row.get(1)?;
        let no_data: i64 = row.get(2)?;
        if let Some(rule) = rules.iter_mut().find(|rule| rule.id == id) {
            rule.firing = firing != 0;
            rule.no_data = no_data != 0;
        }
    }
    Ok(())
}

pub fn silence_rule(conn: &Connection, id: &str, until_ms: i64) -> Result<AlertRule, RuleError> {
    let mut rules = list_rules(conn)?;
    let Some(rule) = rules.iter_mut().find(|rule| rule.id == id) else {
        return Err(RuleError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
    };
    rule.params.silence_until_ms = Some(until_ms);
    rule.updated_at_ms = until_ms;
    let saved = rule.clone();
    upsert_rule(conn, &saved)?;
    Ok(saved)
}

pub fn upsert_rule(conn: &Connection, rule: &AlertRule) -> Result<(), RuleError> {
    let params_json = serde_json::to_string(&rule.params).map_err(|e| {
        RuleError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
    })?;
    let kind = rule.kind.as_str();
    conn.execute(
        "INSERT INTO alert_rules (id, server_id, kind, name, enabled, params_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
            server_id = excluded.server_id,
            kind = excluded.kind,
            name = excluded.name,
            enabled = excluded.enabled,
            params_json = excluded.params_json,
            updated_at_ms = excluded.updated_at_ms",
        params![
            rule.id,
            rule.server_id,
            kind,
            rule.name,
            if rule.enabled { 1 } else { 0 },
            params_json,
            rule.created_at_ms,
            rule.updated_at_ms
        ],
    )?;
    Ok(())
}

pub fn delete_rule(conn: &Connection, id: &str) -> Result<(), RuleError> {
    conn.execute("DELETE FROM alert_rules WHERE id = ?1", params![id])?;
    conn.execute(
        "DELETE FROM alert_rule_state WHERE rule_id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn new_rule_id() -> String {
    Uuid::new_v4().to_string()
}

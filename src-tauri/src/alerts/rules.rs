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
            _ => None,
        }
    }
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
            PRIMARY KEY (rule_id, server_id)
        );
        ",
    )?;
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
            params: serde_json::from_str(&params_json).unwrap_or(RuleParams {
                threshold: 90.0,
                clear_threshold: None,
                duration_secs: 0,
                metric_key: None,
                endpoint_label: None,
            }),
            created_at_ms: row.get(6)?,
            updated_at_ms: row.get(7)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(RuleError::from)
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

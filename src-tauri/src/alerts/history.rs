use rusqlite::{params, Connection};
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertEventRecord {
    pub id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub server_id: String,
    pub metric_key: Option<String>,
    pub severity: String,
    pub message: String,
    pub value: f64,
    pub threshold: f64,
    pub fired_at_ms: i64,
    pub resolved_at_ms: Option<i64>,
}

pub struct AlertHistoryStore;

impl AlertHistoryStore {
    pub fn migrate(conn: &Connection) -> Result<(), HistoryError> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS alert_events (
                id TEXT PRIMARY KEY,
                rule_id TEXT NOT NULL,
                rule_name TEXT NOT NULL,
                server_id TEXT NOT NULL,
                metric_key TEXT,
                severity TEXT NOT NULL,
                message TEXT NOT NULL,
                value REAL NOT NULL,
                threshold REAL NOT NULL,
                fired_at_ms INTEGER NOT NULL,
                resolved_at_ms INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_alert_events_server
                ON alert_events (server_id, fired_at_ms DESC);
            ",
        )?;
        Ok(())
    }

    pub fn insert_firing(
        conn: &Connection,
        rule_id: &str,
        rule_name: &str,
        server_id: &str,
        metric_key: Option<&str>,
        message: &str,
        value: f64,
        threshold: f64,
        fired_at_ms: i64,
    ) -> Result<String, HistoryError> {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO alert_events (id, rule_id, rule_name, server_id, metric_key, severity, message, value, threshold, fired_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, 'critical', ?6, ?7, ?8, ?9)",
            params![
                id,
                rule_id,
                rule_name,
                server_id,
                metric_key,
                message,
                value,
                threshold,
                fired_at_ms
            ],
        )?;
        Ok(id)
    }

    pub fn resolve_open(
        conn: &Connection,
        rule_id: &str,
        server_id: &str,
        resolved_at_ms: i64,
    ) -> Result<(), HistoryError> {
        conn.execute(
            "UPDATE alert_events SET resolved_at_ms = ?3
             WHERE rule_id = ?1 AND server_id = ?2 AND resolved_at_ms IS NULL",
            params![rule_id, server_id, resolved_at_ms],
        )?;
        Ok(())
    }

    pub fn list_history(
        conn: &Connection,
        server_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<AlertEventRecord>, HistoryError> {
        let sql = if server_id.is_some() {
            "SELECT id, rule_id, rule_name, server_id, metric_key, severity, message, value, threshold, fired_at_ms, resolved_at_ms
             FROM alert_events WHERE server_id = ?1 ORDER BY fired_at_ms DESC LIMIT ?2"
        } else {
            "SELECT id, rule_id, rule_name, server_id, metric_key, severity, message, value, threshold, fired_at_ms, resolved_at_ms
             FROM alert_events ORDER BY fired_at_ms DESC LIMIT ?1"
        };

        if let Some(sid) = server_id {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map(params![sid, limit], map_row)?;
            return rows.collect::<Result<Vec<_>, _>>().map_err(HistoryError::from);
        }

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![limit], map_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(HistoryError::from)
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AlertEventRecord> {
    Ok(AlertEventRecord {
        id: row.get(0)?,
        rule_id: row.get(1)?,
        rule_name: row.get(2)?,
        server_id: row.get(3)?,
        metric_key: row.get(4)?,
        severity: row.get(5)?,
        message: row.get(6)?,
        value: row.get(7)?,
        threshold: row.get(8)?,
        fired_at_ms: row.get(9)?,
        resolved_at_ms: row.get(10)?,
    })
}

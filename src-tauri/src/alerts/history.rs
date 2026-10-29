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
pub struct ChartMarker {
    pub id: String,
    pub server_id: String,
    pub ts_ms: i64,
    pub kind: String,
    pub label: String,
    pub detail: String,
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
            CREATE TABLE IF NOT EXISTS chart_markers (
                id TEXT PRIMARY KEY,
                server_id TEXT NOT NULL,
                ts_ms INTEGER NOT NULL,
                kind TEXT NOT NULL,
                label TEXT NOT NULL,
                detail TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_chart_markers_server
                ON chart_markers (server_id, ts_ms);
            ",
        )?;
        Ok(())
    }

    /// One marker per server, kind, and label inside a minute. Stored with alert history.
    pub fn record_marker(
        conn: &Connection,
        server_id: &str,
        ts_ms: i64,
        kind: &str,
        label: &str,
        detail: &str,
    ) -> Result<(), HistoryError> {
        let label = clip(label, 160);
        let detail = clip(detail, 400);
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM chart_markers
                 WHERE server_id = ?1 AND kind = ?2 AND label = ?3 AND ts_ms >= ?4 AND ts_ms <= ?5
                 LIMIT 1",
                params![server_id, kind, label, ts_ms - 60_000, ts_ms + 60_000],
                |row| row.get(0),
            )
            .ok();
        if exists.is_some() {
            return Ok(());
        }
        conn.execute(
            "INSERT INTO chart_markers (id, server_id, ts_ms, kind, label, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![Uuid::new_v4().to_string(), server_id, ts_ms, kind, label, detail],
        )?;
        Ok(())
    }

    pub fn list_markers(
        conn: &Connection,
        server_id: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<ChartMarker>, HistoryError> {
        let mut stmt = conn.prepare(
            "SELECT id, server_id, ts_ms, kind, label, detail FROM chart_markers
             WHERE server_id = ?1 AND ts_ms >= ?2 AND ts_ms <= ?3
             ORDER BY ts_ms ASC LIMIT 500",
        )?;
        let rows = stmt.query_map(params![server_id, start_ms, end_ms], |row| {
            Ok(ChartMarker {
                id: row.get(0)?,
                server_id: row.get(1)?,
                ts_ms: row.get(2)?,
                kind: row.get(3)?,
                label: row.get(4)?,
                detail: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(HistoryError::from)
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

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Journal lines such as `2026-10-26T00:05:00+00:00 host CRON[42]: (deploy) CMD (...)`.
pub fn parse_cron_journal(text: &str) -> Vec<(i64, String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.contains("CRON") {
            continue;
        }
        let Some((ts, rest)) = line.split_once(' ') else {
            continue;
        };
        let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) else {
            continue;
        };
        let detail = rest.trim();
        if detail.is_empty() {
            continue;
        }
        let label = detail.chars().take(80).collect();
        out.push((dt.timestamp_millis(), label, detail.to_string()));
        if out.len() >= 200 {
            break;
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_journal_lines_become_markers_and_repeats_collapse() {
        let text = "2026-10-26T00:05:00+00:00 host CRON[42]: (deploy) CMD (/usr/bin/php /srv/worker.php)\nnoise\n";
        let rows = parse_cron_journal(text);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].2.contains("worker.php"));
        let conn = Connection::open_in_memory().unwrap();
        AlertHistoryStore::migrate(&conn).unwrap();
        AlertHistoryStore::record_marker(&conn, "srv", rows[0].0, "cron", &rows[0].1, &rows[0].2).unwrap();
        AlertHistoryStore::record_marker(&conn, "srv", rows[0].0 + 1_000, "cron", &rows[0].1, &rows[0].2).unwrap();
        let listed = AlertHistoryStore::list_markers(&conn, "srv", 0, i64::MAX).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].kind, "cron");
    }
}

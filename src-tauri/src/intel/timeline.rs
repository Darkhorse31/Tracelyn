//! Events gathered from 15 minutes before and after an alert.

use super::store;
use super::types::{IntelError, TimelineEntry};
use rusqlite::{params, Connection};

pub const SIDE_MS: i64 = 15 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEvent {
    pub ts_ms: i64,
    pub kind: String,
    pub message: String,
}

pub fn order_context(mut rows: Vec<ContextEvent>) -> Vec<ContextEvent> {
    rows.sort_by(|a, b| {
        a.ts_ms
            .cmp(&b.ts_ms)
            .then(a.kind.cmp(&b.kind))
            .then(a.message.cmp(&b.message))
    });
    rows.dedup();
    rows
}

pub fn window(center_ms: i64) -> (i64, i64) {
    (
        center_ms.saturating_sub(SIDE_MS),
        center_ms.saturating_add(SIDE_MS),
    )
}

/// Copy anomalies, new error patterns, restarts, deploys, and audit rows onto the incident.
pub fn fill_incident_window(
    conn: &Connection,
    incident_id: &str,
    server_id: &str,
    center_ms: i64,
) -> Result<(), IntelError> {
    let (start, end) = window(center_ms);
    let mut rows = Vec::new();
    for event in store::list_anomalies(conn, server_id, start, end)? {
        rows.push(ContextEvent {
            ts_ms: event.ts_ms,
            kind: "anomaly".into(),
            message: event.message,
        });
    }
    for entry in store::list_feed_between(conn, server_id, start, end)? {
        rows.push(ContextEvent {
            ts_ms: entry.ts_ms,
            kind: feed_kind(&entry.kind, &entry.message),
            message: entry.message,
        });
    }
    rows.extend(audit_between(conn, server_id, start, end));
    rows.extend(markers_between(conn, server_id, start, end));
    let mut existing = store::list_timeline(conn, incident_id)?;
    for event in order_context(rows) {
        if existing.iter().any(|row| same(row, &event)) {
            continue;
        }
        store::add_timeline(conn, incident_id, event.ts_ms, &event.kind, &event.message)?;
        existing.push(TimelineEntry {
            id: String::new(),
            ts_ms: event.ts_ms,
            kind: event.kind,
            message: event.message,
        });
    }
    Ok(())
}

pub fn push_unique(timeline: &mut Vec<TimelineEntry>, entry: TimelineEntry) {
    if timeline.iter().any(|row| row.ts_ms == entry.ts_ms && row.message == entry.message) {
        return;
    }
    timeline.push(entry);
}

fn same(row: &TimelineEntry, event: &ContextEvent) -> bool {
    row.ts_ms == event.ts_ms && row.message == event.message
}

fn feed_kind(kind: &str, message: &str) -> String {
    if message.starts_with("PM2 restart") {
        "restart".into()
    } else if kind == "error" || message.starts_with("New error pattern") {
        "error".into()
    } else {
        kind.to_string()
    }
}

fn audit_between(conn: &Connection, server_id: &str, start: i64, end: i64) -> Vec<ContextEvent> {
    let mut stmt = match conn.prepare(
        "SELECT ts_ms, target, command FROM audit_log
         WHERE server_id = ?1 AND ts_ms >= ?2 AND ts_ms <= ?3
         ORDER BY ts_ms ASC LIMIT 100",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map(params![server_id, start, end], |row| {
        let ts: i64 = row.get(0)?;
        let target: String = row.get(1)?;
        let command: String = row.get(2)?;
        Ok(ContextEvent {
            ts_ms: ts,
            kind: "audit".into(),
            message: format!("{target}: {command}"),
        })
    });
    match rows {
        Ok(rows) => rows.filter_map(|row| row.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

fn markers_between(conn: &Connection, server_id: &str, start: i64, end: i64) -> Vec<ContextEvent> {
    match crate::alerts::AlertHistoryStore::list_markers(conn, server_id, start, end) {
        Ok(markers) => markers
            .into_iter()
            .filter(|marker| marker.kind != "alert")
            .map(|marker| ContextEvent {
                ts_ms: marker.ts_ms,
                kind: marker.kind,
                message: format!("{} {}", marker.label, marker.detail).trim().to_string(),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_is_ordered_across_the_fifteen_minute_window() {
        let center = 1_000_000_i64;
        let (start, end) = window(center);
        assert_eq!(end - start, 30 * 60 * 1000);
        let rows = order_context(vec![
            ContextEvent { ts_ms: center, kind: "alert".into(), message: "cpu".into() },
            ContextEvent { ts_ms: center - 60_000, kind: "restart".into(), message: "api".into() },
            ContextEvent { ts_ms: center - 5 * 60_000, kind: "deploy".into(), message: "sha".into() },
            ContextEvent { ts_ms: center + 60_000, kind: "audit".into(), message: "restart api".into() },
            ContextEvent { ts_ms: center + 2 * 60_000, kind: "anomaly".into(), message: "spike".into() },
            ContextEvent { ts_ms: center - 10 * 60_000, kind: "error".into(), message: "panic".into() },
            ContextEvent { ts_ms: center - 10 * 60_000, kind: "error".into(), message: "panic".into() },
        ]);
        let kinds: Vec<_> = rows.iter().map(|row| row.kind.as_str()).collect();
        assert_eq!(kinds, ["error", "deploy", "restart", "alert", "audit", "anomaly"]);
        assert!(rows.iter().all(|row| row.ts_ms >= start && row.ts_ms <= end));
    }
}

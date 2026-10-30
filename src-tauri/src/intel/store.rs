use super::types::{
    AnomalyEvent, Channel, DigestSpec, Incident, IntelError, IntelRule, RuleRuntime, SavedView,
    SlotModel, TimelineEntry,
};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

pub fn migrate(conn: &Connection) -> Result<(), IntelError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS intel_rules (
            id TEXT PRIMARY KEY,
            server_id TEXT,
            kind TEXT NOT NULL,
            name TEXT NOT NULL,
            enabled INTEGER NOT NULL,
            severity TEXT NOT NULL,
            params_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS intel_rule_state (
            rule_id TEXT NOT NULL,
            server_id TEXT NOT NULL,
            firing INTEGER NOT NULL,
            condition_since_ms INTEGER,
            clear_since_ms INTEGER,
            last_value REAL,
            last_error TEXT,
            checked_at_ms INTEGER NOT NULL DEFAULT 0,
            last_notify_ms INTEGER,
            PRIMARY KEY (rule_id, server_id)
        );
        CREATE TABLE IF NOT EXISTS intel_channels (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            enabled INTEGER NOT NULL,
            config_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS intel_incidents (
            id TEXT PRIMARY KEY,
            server_id TEXT NOT NULL,
            title TEXT NOT NULL,
            severity TEXT NOT NULL,
            status TEXT NOT NULL,
            opened_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            acked_at_ms INTEGER,
            resolved_at_ms INTEGER,
            snooze_until_ms INTEGER,
            unread INTEGER NOT NULL DEFAULT 1
        );
        CREATE INDEX IF NOT EXISTS idx_intel_incidents_server
            ON intel_incidents (server_id, updated_at_ms DESC);
        CREATE TABLE IF NOT EXISTS intel_timeline (
            id TEXT PRIMARY KEY,
            incident_id TEXT NOT NULL,
            ts_ms INTEGER NOT NULL,
            kind TEXT NOT NULL,
            message TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_intel_timeline
            ON intel_timeline (incident_id, ts_ms);
        CREATE TABLE IF NOT EXISTS intel_feed (
            id TEXT PRIMARY KEY,
            server_id TEXT NOT NULL,
            ts_ms INTEGER NOT NULL,
            kind TEXT NOT NULL,
            message TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_intel_feed
            ON intel_feed (server_id, ts_ms);
        CREATE TABLE IF NOT EXISTS intel_anomalies (
            id TEXT PRIMARY KEY,
            server_id TEXT NOT NULL,
            metric_key TEXT NOT NULL,
            ts_ms INTEGER NOT NULL,
            value REAL NOT NULL,
            baseline REAL NOT NULL,
            z REAL NOT NULL,
            message TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_intel_anomalies
            ON intel_anomalies (server_id, ts_ms DESC);
        CREATE TABLE IF NOT EXISTS intel_baseline (
            server_id TEXT NOT NULL,
            metric_key TEXT NOT NULL,
            slot INTEGER NOT NULL,
            ewma REAL NOT NULL,
            residuals TEXT NOT NULL,
            samples INTEGER NOT NULL,
            PRIMARY KEY (server_id, metric_key, slot)
        );
        CREATE TABLE IF NOT EXISTS intel_log_hit (
            rule_id TEXT NOT NULL,
            line_id TEXT NOT NULL,
            ts_ms INTEGER NOT NULL,
            PRIMARY KEY (rule_id, line_id)
        );
        CREATE TABLE IF NOT EXISTS intel_restarts (
            server_id TEXT NOT NULL,
            app_id TEXT NOT NULL,
            name TEXT NOT NULL,
            restarts INTEGER NOT NULL,
            PRIMARY KEY (server_id, app_id)
        );
        CREATE TABLE IF NOT EXISTS intel_views (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL,
            note TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS intel_digests (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            cadence TEXT NOT NULL,
            hour_utc INTEGER NOT NULL,
            weekday INTEGER NOT NULL,
            channel_id TEXT NOT NULL,
            enabled INTEGER NOT NULL,
            last_sent_ms INTEGER,
            last_attempt_ms INTEGER,
            last_error TEXT
        );
        CREATE TABLE IF NOT EXISTS intel_seen_alert (
            alert_id TEXT PRIMARY KEY,
            ts_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS intel_dedupe (
            fingerprint TEXT PRIMARY KEY,
            ts_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS intel_restart_sample (
            server_id TEXT NOT NULL,
            app_id TEXT NOT NULL,
            ts_ms INTEGER NOT NULL,
            restarts INTEGER NOT NULL,
            PRIMARY KEY (server_id, app_id, ts_ms)
        );
        CREATE TABLE IF NOT EXISTS intel_seen_error (
            server_id TEXT NOT NULL,
            signature TEXT NOT NULL,
            PRIMARY KEY (server_id, signature)
        );
        CREATE TABLE IF NOT EXISTS intel_seen_login (
            server_id TEXT NOT NULL,
            signature TEXT NOT NULL,
            PRIMARY KEY (server_id, signature)
        );
        CREATE TABLE IF NOT EXISTS intel_app_version (
            server_id TEXT NOT NULL,
            app_id TEXT NOT NULL,
            version TEXT NOT NULL,
            PRIMARY KEY (server_id, app_id)
        );
        CREATE TABLE IF NOT EXISTS intel_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        ",
    )?;
    let _ = conn.execute("ALTER TABLE intel_incidents ADD COLUMN kind TEXT NOT NULL DEFAULT 'unknown'", []);
    let _ = conn.execute("ALTER TABLE intel_incidents ADD COLUMN snooze_until_ms INTEGER", []);
    let _ = conn.execute(
        "ALTER TABLE intel_incidents ADD COLUMN unread INTEGER NOT NULL DEFAULT 1",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE intel_rule_state ADD COLUMN last_notify_ms INTEGER",
        [],
    );
    super::incidents::migrate(conn)?;
    Ok(())
}

fn json_err(err: serde_json::Error) -> IntelError {
    IntelError::Bad(err.to_string())
}

pub fn list_rules(conn: &Connection) -> Result<Vec<IntelRule>, IntelError> {
    let mut stmt = conn.prepare(
        "SELECT id, server_id, kind, name, enabled, severity, params_json, created_at_ms, updated_at_ms
         FROM intel_rules ORDER BY name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, i64>(8)?,
        ))
    })?;
    let collected = rows.collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut out = Vec::new();
    for (id, server_id, kind, name, enabled, severity, params_json, created, updated) in collected {
        let firing = conn.query_row(
            "SELECT 1 FROM intel_rule_state WHERE rule_id = ?1 AND firing = 1 LIMIT 1",
            params![id],
            |_| Ok(true),
        ).optional()?.unwrap_or(false);
        out.push(IntelRule {
            id,
            server_id,
            kind,
            name,
            enabled: enabled != 0,
            severity,
            params: serde_json::from_str(&params_json).unwrap_or_default(),
            created_at_ms: created,
            updated_at_ms: updated,
            firing,
        });
    }
    Ok(out)
}

pub fn upsert_rule(conn: &Connection, rule: &IntelRule) -> Result<(), IntelError> {
    let params_json = serde_json::to_string(&rule.params).map_err(json_err)?;
    conn.execute(
        "INSERT INTO intel_rules (id, server_id, kind, name, enabled, severity, params_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            server_id = excluded.server_id,
            kind = excluded.kind,
            name = excluded.name,
            enabled = excluded.enabled,
            severity = excluded.severity,
            params_json = excluded.params_json,
            updated_at_ms = excluded.updated_at_ms",
        params![
            rule.id,
            rule.server_id,
            rule.kind,
            rule.name,
            if rule.enabled { 1 } else { 0 },
            rule.severity,
            params_json,
            rule.created_at_ms,
            rule.updated_at_ms
        ],
    )?;
    Ok(())
}

pub fn delete_rule(conn: &Connection, id: &str) -> Result<(), IntelError> {
    conn.execute("DELETE FROM intel_rules WHERE id = ?1", params![id])?;
    conn.execute("DELETE FROM intel_rule_state WHERE rule_id = ?1", params![id])?;
    conn.execute("DELETE FROM intel_log_hit WHERE rule_id = ?1", params![id])?;
    Ok(())
}

pub fn load_state(conn: &Connection, rule_id: &str, server_id: &str) -> Result<RuleRuntime, IntelError> {
    let row = conn
        .query_row(
            "SELECT firing, condition_since_ms, clear_since_ms, last_value, last_error, checked_at_ms, last_notify_ms
             FROM intel_rule_state WHERE rule_id = ?1 AND server_id = ?2",
            params![rule_id, server_id],
            |row| {
                Ok(RuleRuntime {
                    firing: row.get::<_, i64>(0)? != 0,
                    condition_since_ms: row.get(1)?,
                    clear_since_ms: row.get(2)?,
                    last_value: row.get(3)?,
                    last_error: row.get(4)?,
                    checked_at_ms: row.get(5)?,
                    last_notify_ms: row.get(6)?,
                })
            },
        )
        .optional()?;
    Ok(row.unwrap_or_default())
}

pub fn save_state(conn: &Connection, rule_id: &str, server_id: &str, state: &RuleRuntime) -> Result<(), IntelError> {
    conn.execute(
        "INSERT INTO intel_rule_state
            (rule_id, server_id, firing, condition_since_ms, clear_since_ms, last_value, last_error, checked_at_ms, last_notify_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(rule_id, server_id) DO UPDATE SET
            firing = excluded.firing,
            condition_since_ms = excluded.condition_since_ms,
            clear_since_ms = excluded.clear_since_ms,
            last_value = excluded.last_value,
            last_error = excluded.last_error,
            checked_at_ms = excluded.checked_at_ms,
            last_notify_ms = excluded.last_notify_ms",
        params![
            rule_id,
            server_id,
            if state.firing { 1 } else { 0 },
            state.condition_since_ms,
            state.clear_since_ms,
            state.last_value,
            state.last_error,
            state.checked_at_ms,
            state.last_notify_ms
        ],
    )?;
    Ok(())
}

pub fn list_channels(conn: &Connection) -> Result<Vec<Channel>, IntelError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, kind, enabled, config_json, created_at_ms FROM intel_channels ORDER BY name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, kind, enabled, config_json, created) = row?;
        out.push(Channel {
            id,
            name,
            kind,
            enabled: enabled != 0,
            config: serde_json::from_str(&config_json).unwrap_or_default(),
            has_secret: false,
            created_at_ms: created,
        });
    }
    Ok(out)
}

pub fn upsert_channel(conn: &Connection, channel: &Channel) -> Result<(), IntelError> {
    let config_json = serde_json::to_string(&channel.config).map_err(json_err)?;
    conn.execute(
        "INSERT INTO intel_channels (id, name, kind, enabled, config_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            kind = excluded.kind,
            enabled = excluded.enabled,
            config_json = excluded.config_json",
        params![
            channel.id,
            channel.name,
            channel.kind,
            if channel.enabled { 1 } else { 0 },
            config_json,
            channel.created_at_ms
        ],
    )?;
    Ok(())
}

pub fn delete_channel(conn: &Connection, id: &str) -> Result<(), IntelError> {
    conn.execute("DELETE FROM intel_channels WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn list_incidents(conn: &Connection, server_id: Option<&str>) -> Result<Vec<Incident>, IntelError> {
    let mut stmt = conn.prepare(
        "SELECT id, server_id, title, severity, status, opened_at_ms, updated_at_ms, acked_at_ms, resolved_at_ms, snooze_until_ms, unread, kind
         FROM intel_incidents
         WHERE (?1 IS NULL OR server_id = ?1)
         ORDER BY updated_at_ms DESC
         ",
    )?;
    let rows = stmt.query_map(params![server_id], incident_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(IntelError::from)
}

fn incident_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Incident> {
    Ok(Incident {
        kind: row.get(11)?,
        id: row.get(0)?,
        server_id: row.get(1)?,
        title: row.get(2)?,
        severity: row.get(3)?,
        status: row.get(4)?,
        opened_at_ms: row.get(5)?,
        updated_at_ms: row.get(6)?,
        acked_at_ms: row.get(7)?,
        resolved_at_ms: row.get(8)?,
        snooze_until_ms: row.get(9)?,
        unread: row.get::<_, i64>(10)? != 0,
    })
}

pub fn get_incident(conn: &Connection, id: &str) -> Result<Option<Incident>, IntelError> {
    let row = conn
        .query_row(
            "SELECT id, server_id, title, severity, status, opened_at_ms, updated_at_ms, acked_at_ms, resolved_at_ms, snooze_until_ms, unread, kind
             FROM intel_incidents WHERE id = ?1",
            params![id],
            incident_row,
        )
        .optional()?;
    Ok(row)
}

pub fn open_incident(
    conn: &Connection,
    server_id: &str,
    title: &str,
    severity: &str,
    ts_ms: i64,
) -> Result<String, IntelError> {
    if let Some(existing) = conn
        .query_row(
            "SELECT id FROM intel_incidents
             WHERE server_id = ?1 AND status != 'resolved' AND updated_at_ms >= ?2
             ORDER BY updated_at_ms DESC LIMIT 1",
            params![server_id, ts_ms - super::logic::JOIN_WINDOW_MS],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        let current = get_incident(conn, &existing)?.ok_or_else(|| IntelError::Bad("incident missing".into()))?;
        let severity = super::logic::max_severity(&current.severity, severity);
        conn.execute(
            "UPDATE intel_incidents SET severity = ?1, updated_at_ms = ?2, unread = 1 WHERE id = ?3",
            params![severity, ts_ms, existing],
        )?;
        return Ok(existing);
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO intel_incidents
            (id, server_id, title, severity, status, opened_at_ms, updated_at_ms, acked_at_ms, resolved_at_ms)
         VALUES (?1, ?2, ?3, ?4, 'open', ?5, ?5, NULL, NULL)",
        params![id, server_id, title, severity, ts_ms],
    )?;
    Ok(id)
}

pub fn touch_incident(conn: &Connection, id: &str, ts_ms: i64) -> Result<(), IntelError> {
    conn.execute(
        "UPDATE intel_incidents SET updated_at_ms = ?1 WHERE id = ?2",
        params![ts_ms, id],
    )?;
    Ok(())
}

pub fn set_incident_status(
    conn: &Connection,
    id: &str,
    status: &str,
    ts_ms: i64,
) -> Result<(), IntelError> {
    match status {
        "ack" => conn.execute(
            "UPDATE intel_incidents
             SET status = 'ack', acked_at_ms = ?1, updated_at_ms = ?1
             WHERE id = ?2 AND status != 'resolved'",
            params![ts_ms, id],
        )?,
        "resolved" => conn.execute(
            "UPDATE intel_incidents
             SET status = 'resolved', resolved_at_ms = ?1, updated_at_ms = ?1
             WHERE id = ?2 AND status != 'resolved'",
            params![ts_ms, id],
        )?,
        _ => return Err(IntelError::Bad("unknown incident status".into())),
    };
    Ok(())
}

pub fn add_timeline(
    conn: &Connection,
    incident_id: &str,
    ts_ms: i64,
    kind: &str,
    message: &str,
) -> Result<(), IntelError> {
    conn.execute(
        "INSERT INTO intel_timeline (id, incident_id, ts_ms, kind, message) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![Uuid::new_v4().to_string(), incident_id, ts_ms, kind, message],
    )?;
    Ok(())
}

pub fn list_timeline(conn: &Connection, incident_id: &str) -> Result<Vec<TimelineEntry>, IntelError> {
    let mut stmt = conn.prepare(
        "SELECT id, ts_ms, kind, message FROM intel_timeline WHERE incident_id = ?1 ORDER BY ts_ms ASC LIMIT 500",
    )?;
    let rows = stmt.query_map(params![incident_id], |row| {
        Ok(TimelineEntry {
            id: row.get(0)?,
            ts_ms: row.get(1)?,
            kind: row.get(2)?,
            message: row.get(3)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(IntelError::from)
}

pub fn add_feed(conn: &Connection, server_id: &str, ts_ms: i64, kind: &str, message: &str) -> Result<(), IntelError> {
    conn.execute(
        "INSERT INTO intel_feed (id, server_id, ts_ms, kind, message) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![Uuid::new_v4().to_string(), server_id, ts_ms, kind, message],
    )?;
    Ok(())
}

pub fn list_feed_between(
    conn: &Connection,
    server_id: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<TimelineEntry>, IntelError> {
    let mut stmt = conn.prepare(
        "SELECT id, ts_ms, kind, message FROM intel_feed
         WHERE server_id = ?1 AND ts_ms >= ?2 AND ts_ms <= ?3
         ORDER BY ts_ms ASC LIMIT 200",
    )?;
    let rows = stmt.query_map(params![server_id, start_ms, end_ms], |row| {
        Ok(TimelineEntry {
            id: row.get(0)?,
            ts_ms: row.get(1)?,
            kind: row.get(2)?,
            message: row.get(3)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(IntelError::from)
}

pub fn latest_anomaly_ts(conn: &Connection, server_id: &str, metric_key: &str) -> Result<Option<i64>, IntelError> {
    let ts = conn
        .query_row(
            "SELECT ts_ms FROM intel_anomalies WHERE server_id = ?1 AND metric_key = ?2 ORDER BY ts_ms DESC LIMIT 1",
            params![server_id, metric_key],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    Ok(ts)
}

pub fn add_anomaly(conn: &Connection, event: &AnomalyEvent) -> Result<(), IntelError> {
    conn.execute(
        "INSERT INTO intel_anomalies (id, server_id, metric_key, ts_ms, value, baseline, z, message)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            event.id,
            event.server_id,
            event.metric_key,
            event.ts_ms,
            event.value,
            event.baseline,
            event.z,
            event.message
        ],
    )?;
    Ok(())
}

pub fn list_anomalies(
    conn: &Connection,
    server_id: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<AnomalyEvent>, IntelError> {
    let mut stmt = conn.prepare(
        "SELECT id, server_id, metric_key, ts_ms, value, baseline, z, message
         FROM intel_anomalies
         WHERE server_id = ?1 AND ts_ms >= ?2 AND ts_ms <= ?3
         ORDER BY ts_ms DESC LIMIT 200",
    )?;
    let rows = stmt.query_map(params![server_id, start_ms, end_ms], anomaly_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(IntelError::from)
}

fn anomaly_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnomalyEvent> {
    Ok(AnomalyEvent {
        id: row.get(0)?,
        server_id: row.get(1)?,
        metric_key: row.get(2)?,
        ts_ms: row.get(3)?,
        value: row.get(4)?,
        baseline: row.get(5)?,
        z: row.get(6)?,
        message: row.get(7)?,
    })
}

pub fn load_baseline(
    conn: &Connection,
    server_id: &str,
    metric_key: &str,
    slot: u16,
) -> Result<SlotModel, IntelError> {
    let row = conn
        .query_row(
            "SELECT ewma, residuals, samples FROM intel_baseline
             WHERE server_id = ?1 AND metric_key = ?2 AND slot = ?3",
            params![server_id, metric_key, slot],
            |row| Ok((row.get::<_, f64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)),
        )
        .optional()?;
    Ok(match row {
        Some((ewma, residuals, samples)) => SlotModel {
            ewma,
            residuals: parse_residuals(&residuals),
            samples: samples.max(0) as u32,
        },
        None => SlotModel::default(),
    })
}

pub fn save_baseline(
    conn: &Connection,
    server_id: &str,
    metric_key: &str,
    slot: u16,
    model: &SlotModel,
) -> Result<(), IntelError> {
    conn.execute(
        "INSERT INTO intel_baseline (server_id, metric_key, slot, ewma, residuals, samples)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(server_id, metric_key, slot) DO UPDATE SET
            ewma = excluded.ewma,
            residuals = excluded.residuals,
            samples = excluded.samples",
        params![
            server_id,
            metric_key,
            slot,
            model.ewma,
            format_residuals(&model.residuals),
            model.samples
        ],
    )?;
    Ok(())
}

fn format_residuals(values: &[f64]) -> String {
    values.iter().map(|v| format!("{v:.6}")).collect::<Vec<_>>().join(",")
}

fn parse_residuals(raw: &str) -> Vec<f64> {
    if raw.is_empty() {
        return Vec::new();
    }
    raw.split(',').filter_map(|part| part.parse().ok()).collect()
}

pub fn remember_log_hit(conn: &Connection, rule_id: &str, line_id: &str, ts_ms: i64) -> Result<(), IntelError> {
    conn.execute(
        "INSERT OR IGNORE INTO intel_log_hit (rule_id, line_id, ts_ms) VALUES (?1, ?2, ?3)",
        params![rule_id, line_id, ts_ms],
    )?;
    Ok(())
}

pub fn count_log_hits(conn: &Connection, rule_id: &str, since_ms: i64) -> Result<u64, IntelError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM intel_log_hit WHERE rule_id = ?1 AND ts_ms >= ?2",
        params![rule_id, since_ms],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as u64)
}

pub fn prune_log_hits(conn: &Connection, rule_id: &str, before_ms: i64) -> Result<(), IntelError> {
    conn.execute(
        "DELETE FROM intel_log_hit WHERE rule_id = ?1 AND ts_ms < ?2",
        params![rule_id, before_ms],
    )?;
    Ok(())
}

pub fn get_restart(conn: &Connection, server_id: &str, app_id: &str) -> Result<Option<u64>, IntelError> {
    let value = conn
        .query_row(
            "SELECT restarts FROM intel_restarts WHERE server_id = ?1 AND app_id = ?2",
            params![server_id, app_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    Ok(value.map(|v| v.max(0) as u64))
}

pub fn set_restart(
    conn: &Connection,
    server_id: &str,
    app_id: &str,
    name: &str,
    restarts: u64,
) -> Result<(), IntelError> {
    conn.execute(
        "INSERT INTO intel_restarts (server_id, app_id, name, restarts)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(server_id, app_id) DO UPDATE SET name = excluded.name, restarts = excluded.restarts",
        params![server_id, app_id, name, restarts as i64],
    )?;
    Ok(())
}

pub fn claim_alert(conn: &Connection, alert_id: &str, ts_ms: i64) -> Result<bool, IntelError> {
    let n = conn.execute(
        "INSERT OR IGNORE INTO intel_seen_alert (alert_id, ts_ms) VALUES (?1, ?2)",
        params![alert_id, ts_ms],
    )?;
    Ok(n > 0)
}

pub fn recent_open_alerts(
    conn: &Connection,
    server_id: &str,
    since_ms: i64,
) -> Result<Vec<(String, String, String, i64)>, IntelError> {
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'alert_events'",
            [],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if !exists {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT id, rule_name, message, fired_at_ms FROM alert_events
         WHERE server_id = ?1 AND fired_at_ms >= ?2
         ORDER BY fired_at_ms ASC",
    )?;
    let rows = stmt.query_map(params![server_id, since_ms], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(IntelError::from)
}

pub fn list_views(conn: &Connection) -> Result<Vec<SavedView>, IntelError> {
    let mut stmt = conn.prepare("SELECT id, name, path, note FROM intel_views ORDER BY name COLLATE NOCASE")?;
    let rows = stmt.query_map([], |row| {
        Ok(SavedView {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            note: row.get(3)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(IntelError::from)
}

pub fn upsert_view(conn: &Connection, view: &SavedView) -> Result<(), IntelError> {
    conn.execute(
        "INSERT INTO intel_views (id, name, path, note) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, path = excluded.path, note = excluded.note",
        params![view.id, view.name, view.path, view.note],
    )?;
    Ok(())
}

pub fn delete_view(conn: &Connection, id: &str) -> Result<(), IntelError> {
    conn.execute("DELETE FROM intel_views WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn list_digests(conn: &Connection) -> Result<Vec<DigestSpec>, IntelError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, cadence, hour_utc, weekday, channel_id, enabled, last_sent_ms, last_attempt_ms, last_error
         FROM intel_digests ORDER BY name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], digest_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(IntelError::from)
}

fn digest_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DigestSpec> {
    Ok(DigestSpec {
        id: row.get(0)?,
        name: row.get(1)?,
        cadence: row.get(2)?,
        hour_utc: row.get::<_, i64>(3)?.clamp(0, 23) as u8,
        weekday: row.get::<_, i64>(4)?.clamp(0, 6) as u8,
        channel_id: row.get(5)?,
        enabled: row.get::<_, i64>(6)? != 0,
        last_sent_ms: row.get(7)?,
        last_attempt_ms: row.get(8)?,
        last_error: row.get(9)?,
    })
}

pub fn upsert_digest(conn: &Connection, digest: &DigestSpec) -> Result<(), IntelError> {
    conn.execute(
        "INSERT INTO intel_digests
            (id, name, cadence, hour_utc, weekday, channel_id, enabled, last_sent_ms, last_attempt_ms, last_error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            cadence = excluded.cadence,
            hour_utc = excluded.hour_utc,
            weekday = excluded.weekday,
            channel_id = excluded.channel_id,
            enabled = excluded.enabled,
            last_sent_ms = excluded.last_sent_ms,
            last_attempt_ms = excluded.last_attempt_ms,
            last_error = excluded.last_error",
        params![
            digest.id,
            digest.name,
            digest.cadence,
            digest.hour_utc,
            digest.weekday,
            digest.channel_id,
            if digest.enabled { 1 } else { 0 },
            digest.last_sent_ms,
            digest.last_attempt_ms,
            digest.last_error
        ],
    )?;
    Ok(())
}

pub fn delete_digest(conn: &Connection, id: &str) -> Result<(), IntelError> {
    conn.execute("DELETE FROM intel_digests WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn metric_series(
    conn: &Connection,
    server_id: &str,
    metric_key: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<(i64, f64)>, IntelError> {
    let mut stmt = conn.prepare(
        "SELECT ts_ms, value FROM (
            SELECT ts_ms, value FROM metric_raw
            WHERE server_id = ?1 AND metric_key = ?2 AND ts_ms >= ?3 AND ts_ms <= ?4
            ORDER BY ts_ms DESC LIMIT 2000
         ) ORDER BY ts_ms ASC",
    )?;
    let rows = stmt.query_map(params![server_id, metric_key, start_ms, end_ms], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(IntelError::from)
}

pub fn latest_metrics(conn: &Connection, server_id: &str, since_ms: i64) -> Result<Vec<(String, f64)>, IntelError> {
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'metric_raw'",
            [],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if !exists {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT metric_key, value FROM metric_raw
         WHERE server_id = ?1 AND ts_ms >= ?2
         ORDER BY ts_ms ASC",
    )?;
    let rows = stmt.query_map(params![server_id, since_ms], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(IntelError::from)
}

pub fn prune(conn: &Connection) -> Result<(), IntelError> {
    conn.execute_batch(
        "
        DELETE FROM intel_anomalies WHERE id NOT IN (
            SELECT id FROM intel_anomalies ORDER BY ts_ms DESC LIMIT 5000
        );
        DELETE FROM intel_feed WHERE id NOT IN (
            SELECT id FROM intel_feed ORDER BY ts_ms DESC LIMIT 5000
        );
        DELETE FROM intel_seen_alert WHERE alert_id NOT IN (
            SELECT alert_id FROM intel_seen_alert ORDER BY ts_ms DESC LIMIT 10000
        );
        ",
    )?;
    Ok(())
}

pub fn incident_counts(conn: &Connection, since_ms: i64) -> Result<(usize, usize), IntelError> {
    let incidents: i64 = conn.query_row(
        "SELECT COUNT(*) FROM intel_incidents WHERE opened_at_ms >= ?1 OR updated_at_ms >= ?1",
        params![since_ms],
        |row| row.get(0),
    )?;
    let anomalies: i64 = conn.query_row(
        "SELECT COUNT(*) FROM intel_anomalies WHERE ts_ms >= ?1",
        params![since_ms],
        |row| row.get(0),
    )?;
    Ok((incidents.max(0) as usize, anomalies.max(0) as usize))
}

pub fn incidents_since(conn: &Connection, since_ms: i64) -> Result<Vec<Incident>, IntelError> {
    let mut stmt = conn.prepare(
        "SELECT id, server_id, title, severity, status, opened_at_ms, updated_at_ms, acked_at_ms, resolved_at_ms, snooze_until_ms, unread, kind
         FROM intel_incidents
         WHERE opened_at_ms >= ?1 OR updated_at_ms >= ?1
         ORDER BY updated_at_ms DESC LIMIT 20",
    )?;
    let rows = stmt.query_map(params![since_ms], incident_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(IntelError::from)
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn any_firing(conn: &Connection, server_id: &str) -> Result<bool, IntelError> {
    let hit: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM intel_rule_state WHERE server_id = ?1 AND firing = 1 LIMIT 1",
            params![server_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(hit.is_some())
}

pub fn resolve_recent(conn: &Connection, server_id: &str, ts_ms: i64) -> Result<(), IntelError> {
    let since = ts_ms - super::logic::JOIN_WINDOW_MS;
    conn.execute(
        "UPDATE intel_incidents
         SET status = 'resolved', resolved_at_ms = ?1, updated_at_ms = ?1, unread = 0
         WHERE server_id = ?2 AND status != 'resolved' AND updated_at_ms >= ?3",
        params![ts_ms, server_id, since],
    )?;
    Ok(())
}

pub fn snooze_incident(conn: &Connection, id: &str, until_ms: i64) -> Result<(), IntelError> {
    let changed = conn.execute(
        "UPDATE intel_incidents
         SET snooze_until_ms = ?1, updated_at_ms = ?2
         WHERE id = ?3 AND status != 'resolved'",
        params![until_ms, until_ms, id],
    )?;
    if changed == 0 {
        return Err(IntelError::Bad("incident is not open".into()));
    }
    Ok(())
}

pub fn incident_snoozed(conn: &Connection, id: &str, ts_ms: i64) -> Result<bool, IntelError> {
    let until: Option<i64> = conn
        .query_row(
            "SELECT snooze_until_ms FROM intel_incidents WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(until.is_some_and(|until| until > ts_ms))
}

pub fn unread_count(conn: &Connection) -> Result<i64, IntelError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM intel_incidents WHERE unread = 1 AND status != 'resolved'",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}

pub fn mark_alerts_read(conn: &Connection) -> Result<(), IntelError> {
    conn.execute(
        "UPDATE intel_incidents SET unread = 0 WHERE unread = 1",
        [],
    )?;
    Ok(())
}

pub fn admit_fingerprint(
    conn: &Connection,
    fingerprint: &str,
    ts_ms: i64,
    window_ms: i64,
) -> Result<bool, IntelError> {
    let last: Option<i64> = conn
        .query_row(
            "SELECT ts_ms FROM intel_dedupe WHERE fingerprint = ?1",
            params![fingerprint],
            |row| row.get(0),
        )
        .optional()?;
    if !super::policy::dedupe_admit(last, ts_ms, window_ms) {
        return Ok(false);
    }
    conn.execute(
        "INSERT INTO intel_dedupe (fingerprint, ts_ms) VALUES (?1, ?2)
         ON CONFLICT(fingerprint) DO UPDATE SET ts_ms = excluded.ts_ms",
        params![fingerprint, ts_ms],
    )?;
    Ok(true)
}

pub fn record_restart_sample(
    conn: &Connection,
    server_id: &str,
    app_id: &str,
    ts_ms: i64,
    restarts: u64,
) -> Result<(), IntelError> {
    conn.execute(
        "INSERT INTO intel_restart_sample (server_id, app_id, ts_ms, restarts) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(server_id, app_id, ts_ms) DO UPDATE SET restarts = excluded.restarts",
        params![server_id, app_id, ts_ms, restarts as i64],
    )?;
    let keep = ts_ms - 86_400_000;
    conn.execute(
        "DELETE FROM intel_restart_sample WHERE server_id = ?1 AND app_id = ?2 AND ts_ms < ?3",
        params![server_id, app_id, keep],
    )?;
    Ok(())
}

pub fn restart_samples(
    conn: &Connection,
    server_id: &str,
    app_id: &str,
    since_ms: i64,
) -> Result<Vec<(i64, u64)>, IntelError> {
    let mut stmt = conn.prepare(
        "SELECT ts_ms, restarts FROM intel_restart_sample
         WHERE server_id = ?1 AND app_id = ?2 AND ts_ms >= ?3
         ORDER BY ts_ms ASC",
    )?;
    let rows = stmt.query_map(params![server_id, app_id, since_ms], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)? as u64))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(IntelError::from)
}

pub fn remember_error(conn: &Connection, server_id: &str, signature: &str) -> Result<bool, IntelError> {
    let inserted = conn.execute(
        "INSERT INTO intel_seen_error (server_id, signature) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
        params![server_id, signature],
    )?;
    Ok(inserted == 1)
}

pub fn has_any_error(conn: &Connection, server_id: &str) -> Result<bool, IntelError> {
    let hit: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM intel_seen_error WHERE server_id = ?1 LIMIT 1",
            params![server_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(hit.is_some())
}

pub fn remember_login(conn: &Connection, server_id: &str, signature: &str) -> Result<bool, IntelError> {
    let inserted = conn.execute(
        "INSERT INTO intel_seen_login (server_id, signature) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
        params![server_id, signature],
    )?;
    Ok(inserted == 1)
}

pub fn has_any_login(conn: &Connection, server_id: &str) -> Result<bool, IntelError> {
    let hit: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM intel_seen_login WHERE server_id = ?1 LIMIT 1",
            params![server_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(hit.is_some())
}

pub fn take_version(conn: &Connection, server_id: &str, app_id: &str, version: &str) -> Result<Option<String>, IntelError> {
    let previous: Option<String> = conn
        .query_row(
            "SELECT version FROM intel_app_version WHERE server_id = ?1 AND app_id = ?2",
            params![server_id, app_id],
            |row| row.get(0),
        )
        .optional()?;
    if !version.is_empty() {
        conn.execute(
            "INSERT INTO intel_app_version (server_id, app_id, version) VALUES (?1, ?2, ?3)
             ON CONFLICT(server_id, app_id) DO UPDATE SET version = excluded.version",
            params![server_id, app_id, version],
        )?;
    }
    Ok(previous)
}

pub fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>, IntelError> {
    conn.query_row(
        "SELECT value FROM intel_meta WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(IntelError::from)
}

pub fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<(), IntelError> {
    conn.execute(
        "INSERT INTO intel_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn sound_enabled(conn: &Connection) -> Result<bool, IntelError> {
    Ok(meta_get(conn, "sound")?.map(|v| v != "0").unwrap_or(true))
}

pub fn seed_defaults(conn: &Connection, now_ms: i64) -> Result<(), IntelError> {
    if meta_get(conn, "alert_templates_v1")?.is_some() {
        return Ok(());
    }
    let builtins = [
        ("App crashed or exited", "app_crash", "critical", 1.0, 0u64, 0u64, 900u64),
        ("App restart loop", "restart_loop", "critical", 5.0, 0, 600, 900),
        ("App is down", "app_down", "critical", 0.0, 60, 0, 300),
    ];
    for (name, kind, severity, threshold, duration, window, cooldown) in builtins {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM intel_rules WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )?;
        if exists > 0 {
            continue;
        }
        let mut params = super::types::RuleParams::default();
        params.threshold = threshold;
        params.duration_secs = duration;
        params.window_secs = if window == 0 { 60 } else { window };
        params.cooldown_secs = cooldown;
        params.clear_duration_secs = 30;
        if kind == "app_down" {
            params.clear_threshold = Some(0.5);
        }
        let rule = super::types::IntelRule {
            id: new_id(),
            server_id: None,
            kind: kind.into(),
            name: name.into(),
            enabled: true,
            severity: severity.into(),
            params,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            firing: false,
        };
        upsert_rule(conn, &rule)?;
    }
    let channels: i64 = conn.query_row("SELECT COUNT(*) FROM intel_channels", [], |row| row.get(0))?;
    if channels == 0 {
        upsert_channel(
            conn,
            &super::types::Channel {
                id: "channel-desktop".into(),
                name: "Desktop notifications".into(),
                kind: "windows".into(),
                enabled: true,
                config: super::types::ChannelConfig::default(),
                has_secret: false,
                created_at_ms: now_ms,
            },
        )?;
    }
    meta_set(conn, "alert_templates_v1", "1")?;
    Ok(())
}

pub fn anomalies_as_timeline(
    conn: &Connection,
    server_id: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<TimelineEntry>, IntelError> {
    let events = list_anomalies(conn, server_id, start_ms, end_ms)?;
    Ok(events
        .into_iter()
        .map(|event| TimelineEntry {
            id: event.id,
            ts_ms: event.ts_ms,
            kind: "anomaly".into(),
            message: event.message,
        })
        .collect())
}

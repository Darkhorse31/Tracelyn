use crate::insight::detect::Detection;
use crate::insight::map::{build_service_map, order_spans, EdgeSpan};
use crate::insight::model::*;
use crate::insight::otel::{ParsedLog, ParsedMetric, ParsedSpan};
use crate::insight::slowlog::SlowDraft;
use crate::insight::stack::ErrorDraft;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{Map, Value};

const MAX_SPANS_SERVER: i64 = 100_000;
const MAX_SPANS_GLOBAL: i64 = 200_000;
const MAX_ISSUES: i64 = 5_000;
const MAX_SLOW: i64 = 2_000;
const MAX_LOGS: i64 = 20_000;
const TRACE_LIST_LIMIT: usize = 200;
const TRACE_SPAN_LIMIT: usize = 5_000;
const MAP_SPAN_LIMIT: usize = 20_000;

pub fn migrate(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS insight_meta (
            server_id TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            PRIMARY KEY (server_id, key)
        );
        CREATE TABLE IF NOT EXISTS otel_paths (
            server_id TEXT NOT NULL,
            app_id TEXT NOT NULL,
            path TEXT NOT NULL,
            PRIMARY KEY (server_id, app_id, path)
        );
        CREATE TABLE IF NOT EXISTS slowlog_paths (
            server_id TEXT NOT NULL,
            engine TEXT NOT NULL,
            path TEXT NOT NULL,
            source TEXT NOT NULL,
            logging_on INTEGER NOT NULL,
            is_dir INTEGER NOT NULL,
            hint TEXT NOT NULL,
            snippet TEXT NOT NULL,
            PRIMARY KEY (server_id, engine)
        );
        CREATE TABLE IF NOT EXISTS spans (
            server_id TEXT NOT NULL,
            trace_id TEXT NOT NULL,
            span_id TEXT NOT NULL,
            parent_span_id TEXT,
            service TEXT NOT NULL,
            operation TEXT NOT NULL,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            start_ms INTEGER NOT NULL,
            duration_us INTEGER NOT NULL,
            app_id TEXT,
            peer TEXT,
            attributes_json TEXT NOT NULL,
            events_json TEXT NOT NULL,
            PRIMARY KEY (server_id, trace_id, span_id)
        );
        CREATE INDEX IF NOT EXISTS idx_spans_time ON spans (server_id, start_ms);
        CREATE INDEX IF NOT EXISTS idx_spans_trace ON spans (server_id, trace_id);
        CREATE TABLE IF NOT EXISTS file_cursors (
            server_id TEXT NOT NULL,
            path TEXT NOT NULL,
            offset INTEGER NOT NULL,
            PRIMARY KEY (server_id, path)
        );
        CREATE TABLE IF NOT EXISTS otel_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            server_id TEXT NOT NULL,
            ts_ms INTEGER NOT NULL,
            trace_id TEXT,
            span_id TEXT,
            service TEXT NOT NULL,
            severity TEXT NOT NULL,
            body TEXT NOT NULL,
            app_id TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_otel_logs_trace ON otel_logs (server_id, trace_id, ts_ms);
        CREATE TABLE IF NOT EXISTS error_issues (
            server_id TEXT NOT NULL,
            fingerprint TEXT NOT NULL,
            app_id TEXT,
            title TEXT NOT NULL,
            language TEXT NOT NULL,
            first_seen_ms INTEGER NOT NULL,
            last_seen_ms INTEGER NOT NULL,
            count INTEGER NOT NULL,
            state TEXT NOT NULL,
            sample_message TEXT NOT NULL,
            sample_stack TEXT NOT NULL,
            sample_traces_json TEXT NOT NULL,
            sparkline_json TEXT NOT NULL,
            PRIMARY KEY (server_id, fingerprint)
        );
        CREATE TABLE IF NOT EXISTS slow_queries (
            server_id TEXT NOT NULL,
            engine TEXT NOT NULL,
            fingerprint TEXT NOT NULL,
            normalized TEXT NOT NULL,
            app_id TEXT,
            count INTEGER NOT NULL,
            total_ms REAL NOT NULL,
            rows_examined INTEGER NOT NULL,
            last_seen_ms INTEGER NOT NULL,
            sample TEXT NOT NULL,
            durations_json TEXT NOT NULL,
            PRIMARY KEY (server_id, engine, fingerprint)
        );
        ",
    )?;
    let _ = conn.execute(
        "ALTER TABLE otel_paths ADD COLUMN source TEXT NOT NULL DEFAULT 'manual'",
        [],
    );
    Ok(())
}

pub fn prune(conn: &Connection, now_ms: i64, retention_ms: i64) -> Result<(), rusqlite::Error> {
    if retention_ms > 0 {
        let cutoff = now_ms - retention_ms;
        conn.execute("DELETE FROM spans WHERE start_ms < ?1", params![cutoff])?;
        conn.execute("DELETE FROM otel_logs WHERE ts_ms < ?1", params![cutoff])?;
        conn.execute("DELETE FROM error_issues WHERE last_seen_ms < ?1", params![cutoff])?;
        conn.execute("DELETE FROM slow_queries WHERE last_seen_ms < ?1", params![cutoff])?;
    }
    cap_table(conn, "spans", "start_ms", "server_id", MAX_SPANS_SERVER)?;
    conn.execute(
        "DELETE FROM spans WHERE rowid IN (SELECT rowid FROM spans ORDER BY start_ms DESC LIMIT -1 OFFSET ?1)",
        params![MAX_SPANS_GLOBAL],
    )?;
    cap_table(conn, "otel_logs", "ts_ms", "server_id", MAX_LOGS)?;
    cap_table(conn, "error_issues", "last_seen_ms", "server_id", MAX_ISSUES)?;
    cap_table(conn, "slow_queries", "last_seen_ms", "server_id", MAX_SLOW)?;
    Ok(())
}

fn cap_table(conn: &Connection, table: &str, time_col: &str, server_col: &str, keep: i64) -> Result<(), rusqlite::Error> {
    let sql = format!("SELECT DISTINCT {server_col} FROM {table}");
    let mut stmt = conn.prepare(&sql)?;
    let ids: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for id in ids {
        let delete = format!(
            "DELETE FROM {table} WHERE rowid IN (SELECT rowid FROM {table} WHERE {server_col} = ?1 ORDER BY {time_col} DESC LIMIT -1 OFFSET ?2)"
        );
        conn.execute(&delete, params![id, keep])?;
    }
    Ok(())
}

pub fn watched_files(conn: &Connection, server_id: &str) -> Result<Vec<WatchedFile>, rusqlite::Error> {
    let mut files = Vec::new();
    let mut stmt = conn.prepare("SELECT app_id, path FROM otel_paths WHERE server_id = ?1")?;
    let rows = stmt.query_map(params![server_id], |row| {
        Ok(WatchedFile {
            app_id: row.get(0)?,
            path: row.get(1)?,
            kind: "otel".into(),
            is_dir: false,
        })
    })?;
    for row in rows {
        files.push(row?);
    }
    drop(stmt);
    let mut stmt = conn.prepare(
        "SELECT engine, path, is_dir FROM slowlog_paths WHERE server_id = ?1 AND path != '' AND (logging_on = 1 OR source = 'manual')",
    )?;
    let rows = stmt.query_map(params![server_id], |row| {
        Ok(WatchedFile {
            kind: row.get(0)?,
            path: row.get(1)?,
            is_dir: row.get::<_, i64>(2)? != 0,
            app_id: String::new(),
        })
    })?;
    for row in rows {
        files.push(row?);
    }
    Ok(files)
}

pub fn ingest(
    conn: &Connection,
    server_id: &str,
    spans: &[ParsedSpan],
    logs: &[ParsedLog],
    metrics: &[ParsedMetric],
    errors: &[ErrorDraft],
    slow: &[SlowDraft],
) -> Result<Vec<(String, f64, i64)>, rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    if !spans.is_empty() {
        let mut stmt = tx.prepare(
            "INSERT INTO spans (
                server_id, trace_id, span_id, parent_span_id, service, operation, kind, status,
                start_ms, duration_us, app_id, peer, attributes_json, events_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(server_id, trace_id, span_id) DO UPDATE SET
                parent_span_id = excluded.parent_span_id,
                service = excluded.service,
                operation = excluded.operation,
                kind = excluded.kind,
                status = excluded.status,
                start_ms = excluded.start_ms,
                duration_us = excluded.duration_us,
                app_id = excluded.app_id,
                peer = excluded.peer,
                attributes_json = excluded.attributes_json,
                events_json = excluded.events_json",
        )?;
        for span in spans {
            stmt.execute(params![
                server_id,
                span.trace_id,
                span.span_id,
                span.parent_span_id,
                span.service,
                span.operation,
                span.kind,
                span.status,
                span.start_ms,
                span.duration_us,
                span.app_id,
                span.peer,
                json_text(&span.attributes),
                json_text(&span.events),
            ])?;
        }
        drop(stmt);
    }
    if !logs.is_empty() {
        let mut stmt = tx.prepare(
            "INSERT INTO otel_logs (server_id, ts_ms, trace_id, span_id, service, severity, body, app_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for log in logs {
            stmt.execute(params![
                server_id,
                log.ts_ms,
                log.trace_id,
                log.span_id,
                log.service,
                log.severity,
                log.body,
                log.app_id,
            ])?;
        }
        drop(stmt);
    }
    for draft in errors {
        upsert_error(&tx, server_id, draft)?;
    }
    for draft in slow {
        upsert_slow(&tx, server_id, draft)?;
    }
    tx.commit()?;
    let mut samples = Vec::new();
    for metric in metrics {
        samples.push((format!("otel.{}", metric.name), metric.value, metric.ts_ms));
    }
    Ok(samples)
}

fn upsert_error(conn: &Connection, server_id: &str, draft: &ErrorDraft) -> Result<(), rusqlite::Error> {
    let ts = if draft.ts_ms > 0 { draft.ts_ms } else { now_ms() };
    let existing = conn
        .query_row(
            "SELECT count, first_seen_ms, state, sparkline_json, sample_traces_json FROM error_issues WHERE server_id = ?1 AND fingerprint = ?2",
            params![server_id, draft.fingerprint],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    let mut spark = existing
        .as_ref()
        .map(|row| parse_map(&row.3))
        .unwrap_or_default();
    bump_spark(&mut spark, ts);
    let mut traces = existing
        .as_ref()
        .map(|row| parse_strings(&row.4))
        .unwrap_or_default();
    if let Some(trace_id) = draft.trace_id.as_ref().filter(|id| !id.is_empty()) {
        traces.retain(|id| id != trace_id);
        traces.insert(0, trace_id.clone());
        traces.truncate(5);
    }
    let state = match existing.as_ref().map(|row| row.2.as_str()) {
        Some("ignored") => "ignored",
        Some(_) => "open",
        None => "open",
    };
    let first = existing.as_ref().map(|row| row.1.min(ts)).unwrap_or(ts);
    let count = existing.as_ref().map(|row| row.0 + 1).unwrap_or(1);
    conn.execute(
        "INSERT INTO error_issues (
            server_id, fingerprint, app_id, title, language, first_seen_ms, last_seen_ms, count, state,
            sample_message, sample_stack, sample_traces_json, sparkline_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(server_id, fingerprint) DO UPDATE SET
            app_id = excluded.app_id,
            title = excluded.title,
            language = excluded.language,
            first_seen_ms = excluded.first_seen_ms,
            last_seen_ms = excluded.last_seen_ms,
            count = excluded.count,
            state = excluded.state,
            sample_message = excluded.sample_message,
            sample_stack = excluded.sample_stack,
            sample_traces_json = excluded.sample_traces_json,
            sparkline_json = excluded.sparkline_json",
        params![
            server_id,
            draft.fingerprint,
            draft.app_id,
            draft.title,
            draft.language,
            first,
            ts,
            count,
            state,
            draft.message,
            draft.normalized_stack,
            serde_json::to_string(&traces).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&spark).unwrap_or_else(|_| "{}".into()),
        ],
    )?;
    Ok(())
}

fn upsert_slow(conn: &Connection, server_id: &str, draft: &SlowDraft) -> Result<(), rusqlite::Error> {
    let ts = if draft.ts_ms > 0 { draft.ts_ms } else { now_ms() };
    let existing = conn
        .query_row(
            "SELECT count, total_ms, rows_examined, durations_json, last_seen_ms FROM slow_queries WHERE server_id = ?1 AND engine = ?2 AND fingerprint = ?3",
            params![server_id, draft.engine, draft.fingerprint],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?;
    let mut durations = existing
        .as_ref()
        .map(|row| parse_floats(&row.3))
        .unwrap_or_default();
    durations.push(draft.duration_ms);
    if durations.len() > 128 {
        let extra = durations.len() - 128;
        durations.drain(0..extra);
    }
    let count = existing.as_ref().map(|row| row.0 + 1).unwrap_or(1);
    let total = existing.as_ref().map(|row| row.1).unwrap_or(0.0) + draft.duration_ms;
    let rows = existing.as_ref().map(|row| row.2).unwrap_or(0) + draft.rows_examined;
    let last = existing.as_ref().map(|row| row.4.max(ts)).unwrap_or(ts);
    conn.execute(
        "INSERT INTO slow_queries (
            server_id, engine, fingerprint, normalized, app_id, count, total_ms, rows_examined, last_seen_ms, sample, durations_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(server_id, engine, fingerprint) DO UPDATE SET
            normalized = excluded.normalized,
            app_id = COALESCE(excluded.app_id, slow_queries.app_id),
            count = excluded.count,
            total_ms = excluded.total_ms,
            rows_examined = excluded.rows_examined,
            last_seen_ms = excluded.last_seen_ms,
            sample = excluded.sample,
            durations_json = excluded.durations_json",
        params![
            server_id,
            draft.engine,
            draft.fingerprint,
            draft.normalized,
            draft.app_id,
            count,
            total,
            rows,
            last,
            draft.sample,
            serde_json::to_string(&durations).unwrap_or_else(|_| "[]".into()),
        ],
    )?;
    Ok(())
}

pub fn list_traces(conn: &Connection, query: &TraceQuery) -> Result<Vec<TraceSummary>, rusqlite::Error> {
    let min_us = (query.min_duration_ms.unwrap_or(0.0).max(0.0) * 1000.0) as i64;
    let mut stmt = conn.prepare(
        "SELECT trace_id, service, operation, start_ms, trace_duration_us, status, span_count, app_id, has_error
         FROM (
            SELECT
                trace_id,
                service,
                operation,
                start_ms,
                app_id,
                status,
                COUNT(*) OVER (PARTITION BY trace_id) AS span_count,
                (MAX(start_ms * 1000 + duration_us) OVER (PARTITION BY trace_id))
                    - (MIN(start_ms) OVER (PARTITION BY trace_id) * 1000) AS trace_duration_us,
                MAX(CASE WHEN status = 'error' THEN 1 ELSE 0 END) OVER (PARTITION BY trace_id) AS has_error,
                ROW_NUMBER() OVER (
                    PARTITION BY trace_id
                    ORDER BY CASE WHEN parent_span_id IS NULL OR parent_span_id = '' THEN 0 ELSE 1 END, start_ms
                ) AS rn
            FROM spans
            WHERE server_id = ?1
              AND start_ms >= ?2
              AND start_ms <= ?3
              AND (?4 = '' OR service = ?4)
              AND (?5 = '' OR instr(lower(operation), lower(?5)) > 0)
              AND (?6 = '' OR status = ?6)
              AND (?7 = '' OR trace_id = ?7)
              AND (?8 = '' OR IFNULL(app_id, '') = ?8)
         ) ranked
         WHERE rn = 1 AND trace_duration_us >= ?9
         ORDER BY start_ms DESC
         LIMIT ?10",
    )?;
    let rows = stmt.query_map(
        params![
            query.server_id,
            query.start_ms,
            query.end_ms,
            blank(&query.service),
            blank(&query.operation),
            blank(&query.status),
            blank(&query.trace_id),
            blank(&query.app_id),
            min_us,
            TRACE_LIST_LIMIT as i64,
        ],
        |row| {
            let has_error: i64 = row.get(8)?;
            let status: String = row.get(5)?;
            Ok(TraceSummary {
                trace_id: row.get(0)?,
                service: row.get(1)?,
                operation: row.get(2)?,
                start_ms: row.get(3)?,
                duration_us: row.get(4)?,
                status: if has_error != 0 { "error".into() } else { status },
                span_count: row.get(6)?,
                app_id: row.get(7)?,
            })
        },
    )?;
    rows.collect()
}

pub fn get_trace(conn: &Connection, server_id: &str, trace_id: &str) -> Result<TraceDetail, rusqlite::Error> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM spans WHERE server_id = ?1 AND trace_id = ?2",
        params![server_id, trace_id],
        |row| row.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT span_id, parent_span_id, service, operation, kind, status, start_ms, duration_us, app_id, peer, attributes_json, events_json
         FROM spans WHERE server_id = ?1 AND trace_id = ?2 ORDER BY start_ms ASC LIMIT ?3",
    )?;
    let spans = stmt.query_map(params![server_id, trace_id, TRACE_SPAN_LIMIT as i64], |row| {
        Ok(SpanDetail {
            span_id: row.get(0)?,
            parent_span_id: empty_to_none(row.get(1)?),
            service: row.get(2)?,
            operation: row.get(3)?,
            kind: row.get(4)?,
            status: row.get(5)?,
            start_ms: row.get(6)?,
            duration_us: row.get(7)?,
            app_id: empty_to_none(row.get(8)?),
            peer: empty_to_none(row.get(9)?),
            attributes: parse_json(row.get(10)?),
            events: parse_json(row.get(11)?),
            depth: 0,
            critical: false,
        })
    })?;
    let spans: Vec<_> = spans.collect::<Result<_, _>>()?;
    drop(stmt);
    let (spans, critical_path) = order_spans(spans);
    let mut stmt = conn.prepare(
        "SELECT ts_ms, severity, body, span_id, service FROM otel_logs WHERE server_id = ?1 AND trace_id = ?2 ORDER BY ts_ms ASC LIMIT 100",
    )?;
    let logs = stmt.query_map(params![server_id, trace_id], |row| {
        Ok(RelatedLog {
            ts_ms: row.get(0)?,
            severity: row.get(1)?,
            body: row.get(2)?,
            span_id: empty_to_none(row.get(3)?),
            service: row.get(4)?,
        })
    })?;
    let logs = logs.collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut stmt = conn.prepare(
        "SELECT fingerprint, sample_traces_json FROM error_issues WHERE server_id = ?1 AND instr(sample_traces_json, ?2) > 0 LIMIT 20",
    )?;
    let error_fingerprints = stmt
        .query_map(params![server_id, trace_id], |row| {
            let fingerprint: String = row.get(0)?;
            let traces = parse_strings(&row.get::<_, String>(1)?);
            Ok((fingerprint, traces))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|(fingerprint, traces)| traces.contains(&trace_id.to_string()).then_some(fingerprint))
        .collect();
    Ok(TraceDetail {
        trace_id: trace_id.to_string(),
        spans,
        logs,
        error_fingerprints,
        critical_path,
        truncated: total > TRACE_SPAN_LIMIT as i64,
    })
}

pub fn service_map(conn: &Connection, server_id: &str, start_ms: i64, end_ms: i64) -> Result<ServiceMap, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT span_id, parent_span_id, service, kind, status, duration_us, peer
         FROM spans WHERE server_id = ?1 AND start_ms >= ?2 AND start_ms <= ?3
         ORDER BY start_ms DESC LIMIT ?4",
    )?;
    let rows = stmt.query_map(params![server_id, start_ms, end_ms, MAP_SPAN_LIMIT as i64], |row| {
        Ok(EdgeSpan {
            span_id: row.get(0)?,
            parent_span_id: empty_to_none(row.get(1)?),
            service: row.get(2)?,
            kind: row.get(3)?,
            status: row.get(4)?,
            duration_us: row.get(5)?,
            peer: empty_to_none(row.get(6)?),
        })
    })?;
    let spans: Vec<_> = rows.collect::<Result<_, _>>()?;
    let truncated = spans.len() == MAP_SPAN_LIMIT;
    let mut map = build_service_map(&spans, end_ms.saturating_sub(start_ms));
    map.truncated = truncated;
    Ok(map)
}

pub fn list_errors(conn: &Connection, server_id: &str, app_id: Option<&str>, now_ms: i64) -> Result<Vec<ErrorIssue>, rusqlite::Error> {
    let app = app_id.unwrap_or("");
    let mut stmt = conn.prepare(
        "SELECT fingerprint, app_id, title, language, first_seen_ms, last_seen_ms, count, state, sample_message, sample_stack, sample_traces_json, sparkline_json
         FROM error_issues WHERE server_id = ?1 AND (?2 = '' OR IFNULL(app_id, '') = ?2)
         ORDER BY last_seen_ms DESC LIMIT 500",
    )?;
    let rows = stmt.query_map(params![server_id, app], |row| {
        let spark = parse_map(&row.get::<_, String>(11)?);
        Ok(ErrorIssue {
            fingerprint: row.get(0)?,
            app_id: empty_to_none(row.get(1)?),
            title: row.get(2)?,
            language: row.get(3)?,
            first_seen_ms: row.get(4)?,
            last_seen_ms: row.get(5)?,
            count: row.get(6)?,
            state: row.get(7)?,
            sample_message: row.get(8)?,
            sample_stack: row.get(9)?,
            trace_ids: parse_strings(&row.get::<_, String>(10)?),
            sparkline: sparkline_24(&spark, now_ms),
        })
    })?;
    rows.collect()
}

pub fn set_error_state(conn: &Connection, server_id: &str, fingerprint: &str, state: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE error_issues SET state = ?3 WHERE server_id = ?1 AND fingerprint = ?2",
        params![server_id, fingerprint, state],
    )?;
    Ok(())
}

pub fn list_slow(conn: &Connection, server_id: &str, engine: Option<&str>) -> Result<Vec<SlowQueryRow>, rusqlite::Error> {
    let engine = engine.unwrap_or("");
    let mut stmt = conn.prepare(
        "SELECT engine, fingerprint, normalized, app_id, count, total_ms, rows_examined, last_seen_ms, sample, durations_json
         FROM slow_queries WHERE server_id = ?1 AND (?2 = '' OR engine = ?2)
         ORDER BY total_ms DESC LIMIT 300",
    )?;
    let rows = stmt.query_map(params![server_id, engine], |row| {
        let count: i64 = row.get(4)?;
        let total: f64 = row.get(5)?;
        let rows_examined: i64 = row.get(6)?;
        let mut durations = parse_floats(&row.get::<_, String>(9)?);
        Ok(SlowQueryRow {
            engine: row.get(0)?,
            fingerprint: row.get(1)?,
            normalized: row.get(2)?,
            app_id: empty_to_none(row.get(3)?),
            count,
            total_ms: total,
            avg_ms: if count == 0 { 0.0 } else { total / count as f64 },
            p95_ms: crate::insight::map::percentile_95(&mut durations),
            rows_examined_avg: if count == 0 { 0.0 } else { rows_examined as f64 / count as f64 },
            last_seen_ms: row.get(7)?,
            sample: row.get(8)?,
        })
    })?;
    rows.collect()
}

pub fn setup(conn: &Connection, server_id: &str) -> Result<InsightSetup, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT app_id, path FROM otel_paths WHERE server_id = ?1 ORDER BY path")?;
    let paths = stmt
        .query_map(params![server_id], |row| {
            Ok(OtelPathRow {
                app_id: row.get(0)?,
                path: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let suggestions = get_meta(conn, server_id, "otel_suggestions")
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    let stats = get_meta(conn, server_id, "otel_stats")
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    let mut engines = Vec::new();
    for engine in ["mysql", "postgres", "mongo"] {
        engines.push(load_engine(conn, server_id, engine)?);
    }
    Ok(InsightSetup {
        paths,
        suggestions,
        stats,
        engines,
    })
}

fn load_engine(conn: &Connection, server_id: &str, engine: &str) -> Result<SlowEngineStatus, rusqlite::Error> {
    let row = conn
        .query_row(
            "SELECT path, source, logging_on, is_dir, hint, snippet FROM slowlog_paths WHERE server_id = ?1 AND engine = ?2",
            params![server_id, engine],
            |row| {
                Ok(SlowEngineStatus {
                    engine: engine.to_string(),
                    path: empty_to_none(row.get(0)?),
                    source: row.get(1)?,
                    logging_on: row.get::<_, i64>(2)? != 0,
                    is_dir: row.get::<_, i64>(3)? != 0,
                    hint: row.get(4)?,
                    snippet: row.get(5)?,
                })
            },
        )
        .optional()?;
    Ok(row.unwrap_or_else(|| default_engine(engine)))
}

fn default_engine(engine: &str) -> SlowEngineStatus {
    let (hint, snippet) = match engine {
        "mysql" => (crate::insight::detect::mysql_hint(false, false), crate::insight::detect::mysql_snippet()),
        "postgres" => (
            crate::insight::detect::postgres_hint(false, false),
            crate::insight::detect::postgres_snippet(),
        ),
        _ => (crate::insight::detect::mongo_hint(false, false), crate::insight::detect::mongo_snippet()),
    };
    SlowEngineStatus {
        engine: engine.into(),
        path: None,
        source: "auto".into(),
        logging_on: false,
        is_dir: false,
        hint,
        snippet: snippet.into(),
    }
}

pub fn set_otel_paths(conn: &Connection, server_id: &str, app_id: &str, paths: &[String]) -> Result<(), rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM otel_paths WHERE server_id = ?1 AND app_id = ?2",
        params![server_id, app_id],
    )?;
    let mut stmt = tx.prepare(
        "INSERT INTO otel_paths (server_id, app_id, path, source) VALUES (?1, ?2, ?3, 'manual')",
    )?;
    for path in paths {
        stmt.execute(params![server_id, app_id, path])?;
    }
    drop(stmt);
    tx.commit()?;
    Ok(())
}

pub fn adopt_otel_paths(conn: &Connection, server_id: &str, paths: &[String]) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare(
        "INSERT OR IGNORE INTO otel_paths (server_id, app_id, path, source) VALUES (?1, '', ?2, 'auto')",
    )?;
    for path in paths.iter().take(40) {
        if crate::insight::detect::validate_log_path(path).is_ok() {
            stmt.execute(params![server_id, path])?;
        }
    }
    Ok(())
}

pub fn file_offset(conn: &Connection, server_id: &str, path: &str) -> Result<u64, rusqlite::Error> {
    let offset = conn
        .query_row(
            "SELECT offset FROM file_cursors WHERE server_id = ?1 AND path = ?2",
            params![server_id, path],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0);
    Ok(offset.max(0) as u64)
}

pub fn save_file_offset(conn: &Connection, server_id: &str, path: &str, offset: u64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO file_cursors (server_id, path, offset) VALUES (?1, ?2, ?3)
         ON CONFLICT(server_id, path) DO UPDATE SET offset = excluded.offset",
        params![server_id, path, offset as i64],
    )?;
    Ok(())
}

pub fn merge_otel_report(conn: &Connection, server_id: &str, report: &crate::insight::model::OtelReport) -> Result<(), rusqlite::Error> {
    let mut stats = load_stats(conn, server_id);
    stats.reason = report.reason.clone();
    stats.detail = report.detail.clone();
    stats.sample = report.sample.clone();
    stats.collector = report.collector.clone();
    stats.last_write = report.last_write.clone();
    save_stats(conn, server_id, &stats)
}

fn load_stats(conn: &Connection, server_id: &str) -> OtelStats {
    get_meta(conn, server_id, "otel_stats")
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

pub fn set_slow_path(conn: &Connection, server_id: &str, engine: &str, path: &str) -> Result<(), rusqlite::Error> {
    let current = load_engine(conn, server_id, engine)?;
    conn.execute(
        "INSERT INTO slowlog_paths (server_id, engine, path, source, logging_on, is_dir, hint, snippet)
         VALUES (?1, ?2, ?3, 'manual', 1, 0, ?4, ?5)
         ON CONFLICT(server_id, engine) DO UPDATE SET
            path = excluded.path,
            source = 'manual',
            logging_on = 1,
            is_dir = 0,
            hint = excluded.hint",
        params![server_id, engine, path, current.hint, current.snippet],
    )?;
    Ok(())
}

pub fn clear_slow_path(conn: &Connection, server_id: &str, engine: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM slowlog_paths WHERE server_id = ?1 AND engine = ?2 AND source = 'manual'",
        params![server_id, engine],
    )?;
    Ok(())
}

pub fn save_detection(conn: &Connection, server_id: &str, detection: &Detection) -> Result<(), rusqlite::Error> {
    let json = serde_json::to_string(&detection.suggestions).unwrap_or_else(|_| "[]".into());
    set_meta(conn, server_id, "otel_suggestions", &json)?;
    for engine in &detection.engines {
        let existing = conn
            .query_row(
                "SELECT source FROM slowlog_paths WHERE server_id = ?1 AND engine = ?2",
                params![server_id, engine.engine],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if existing.as_deref() == Some("manual") {
            conn.execute(
                "UPDATE slowlog_paths SET logging_on = ?3, hint = ?4, snippet = ?5 WHERE server_id = ?1 AND engine = ?2",
                params![
                    server_id,
                    engine.engine,
                    i64::from(engine.logging_on),
                    engine.hint,
                    engine.snippet
                ],
            )?;
            continue;
        }
        conn.execute(
            "INSERT INTO slowlog_paths (server_id, engine, path, source, logging_on, is_dir, hint, snippet)
             VALUES (?1, ?2, ?3, 'auto', ?4, ?5, ?6, ?7)
             ON CONFLICT(server_id, engine) DO UPDATE SET
                path = excluded.path,
                source = 'auto',
                logging_on = excluded.logging_on,
                is_dir = excluded.is_dir,
                hint = excluded.hint,
                snippet = excluded.snippet",
            params![
                server_id,
                engine.engine,
                engine.path.clone().unwrap_or_default(),
                i64::from(engine.logging_on),
                i64::from(engine.is_dir),
                engine.hint,
                engine.snippet,
            ],
        )?;
    }
    Ok(())
}

pub fn save_stats(conn: &Connection, server_id: &str, stats: &OtelStats) -> Result<(), rusqlite::Error> {
    let json = serde_json::to_string(stats).unwrap_or_else(|_| "{}".into());
    set_meta(conn, server_id, "otel_stats", &json)
}

fn set_meta(conn: &Connection, server_id: &str, key: &str, value: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO insight_meta (server_id, key, value) VALUES (?1, ?2, ?3)
         ON CONFLICT(server_id, key) DO UPDATE SET value = excluded.value",
        params![server_id, key, value],
    )?;
    Ok(())
}

fn get_meta(conn: &Connection, server_id: &str, key: &str) -> Result<Option<String>, rusqlite::Error> {
    conn.query_row(
        "SELECT value FROM insight_meta WHERE server_id = ?1 AND key = ?2",
        params![server_id, key],
        |row| row.get(0),
    )
    .optional()
}

fn json_text(value: &Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "{}".into());
    if text.len() > 8_000 {
        "{\"truncated\":true}".into()
    } else {
        text
    }
}

fn parse_json(text: String) -> Value {
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

fn parse_map(text: &str) -> Map<String, Value> {
    serde_json::from_str(text).unwrap_or_default()
}

fn parse_strings(text: &str) -> Vec<String> {
    serde_json::from_str(text).unwrap_or_default()
}

fn parse_floats(text: &str) -> Vec<f64> {
    serde_json::from_str(text).unwrap_or_default()
}

fn bump_spark(map: &mut Map<String, Value>, ts_ms: i64) {
    let hour = ts_ms.div_euclid(3_600_000);
    let key = hour.to_string();
    let next = map.get(&key).and_then(|v| v.as_u64()).unwrap_or(0) + 1;
    map.insert(key, Value::from(next));
    let cutoff = hour - 24;
    map.retain(|key, _| key.parse::<i64>().unwrap_or(0) >= cutoff);
}

fn sparkline_24(map: &Map<String, Value>, now_ms: i64) -> Vec<u32> {
    let hour = now_ms.div_euclid(3_600_000);
    (0..24)
        .map(|offset| {
            let key = (hour - 23 + offset).to_string();
            map.get(&key).and_then(|v| v.as_u64()).unwrap_or(0) as u32
        })
        .collect()
}

fn blank(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}

fn empty_to_none(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.is_empty())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn span(trace: &str, id: &str, parent: Option<&str>, start: i64, dur_us: i64, status: &str) -> ParsedSpan {
        ParsedSpan {
            trace_id: trace.into(),
            span_id: id.into(),
            parent_span_id: parent.map(|s| s.into()),
            service: "api".into(),
            operation: if parent.is_none() { "GET /users".into() } else { "SELECT".into() },
            kind: if parent.is_none() { "server".into() } else { "client".into() },
            status: status.into(),
            start_ms: start,
            duration_us: dur_us,
            peer: if parent.is_none() { None } else { Some("mysql shop".into()) },
            attributes: json!({"http.method": "GET"}),
            events: json!([]),
            app_id: Some("app".into()),
        }
    }

    #[test]
    fn stores_queries_and_prunes_traces() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let spans = vec![
            span("t1", "root", None, 1_000, 50_000, "ok"),
            span("t1", "db", Some("root"), 1_010, 80_000, "error"),
        ];
        ingest(&conn, "srv", &spans, &[], &[], &[], &[]).unwrap();
        let listed = list_traces(
            &conn,
            &TraceQuery {
                server_id: "srv".into(),
                start_ms: 0,
                end_ms: 5_000,
                service: None,
                operation: None,
                status: None,
                min_duration_ms: Some(50.0),
                trace_id: None,
                app_id: Some("app".into()),
            },
        )
        .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, "error");
        assert!(listed[0].span_count >= 2);
        let detail = get_trace(&conn, "srv", "t1").unwrap();
        assert_eq!(detail.spans.len(), 2);
        assert_eq!(detail.spans[0].span_id, "root");
        assert!(detail.critical_path.contains(&"db".to_string()));
        let map = service_map(&conn, "srv", 0, 5_000).unwrap();
        assert!(map.edges.iter().any(|e| e.target == "mysql shop"));

        let mut error = crate::insight::stack::fingerprint_text("Error: boom\n    at handler (/app/a.js:1:1)").unwrap();
        error.trace_id = Some("t1".into());
        error.ts_ms = 1_000;
        error.app_id = Some("app".into());
        ingest(&conn, "srv", &[], &[], &[], &[error.clone()], &[]).unwrap();
        ingest(&conn, "srv", &[], &[], &[], &[error], &[]).unwrap();
        let issues = list_errors(&conn, "srv", Some("app"), 1_000).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].count, 2);
        assert_eq!(issues[0].trace_ids, vec!["t1".to_string()]);
        set_error_state(&conn, "srv", &issues[0].fingerprint, "ignored").unwrap();
        let mut again = crate::insight::stack::fingerprint_text("Error: boom\n    at handler (/app/a.js:9:1)").unwrap();
        again.ts_ms = 2_000;
        ingest(&conn, "srv", &[], &[], &[], &[again], &[]).unwrap();
        let issues = list_errors(&conn, "srv", None, 2_000).unwrap();
        assert_eq!(issues[0].state, "ignored");
        assert_eq!(issues[0].count, 3);

        let slow = SlowDraft {
            engine: "mysql".into(),
            normalized: "select ?".into(),
            fingerprint: "abc".into(),
            duration_ms: 12.0,
            rows_examined: 10,
            sample: "select 1".into(),
            ts_ms: 1_000,
            app_id: None,
        };
        let mut slower = slow.clone();
        slower.duration_ms = 40.0;
        ingest(&conn, "srv", &[], &[], &[], &[], &[slow, slower]).unwrap();
        let queries = list_slow(&conn, "srv", Some("mysql")).unwrap();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0].count, 2);
        assert!((queries[0].avg_ms - 26.0).abs() < 0.01);

        prune(&conn, 10_000_000_000, 1_000).unwrap();
        assert!(list_traces(
            &conn,
            &TraceQuery {
                server_id: "srv".into(),
                start_ms: 0,
                end_ms: 10_000_000_000,
                service: None,
                operation: None,
                status: None,
                min_duration_ms: None,
                trace_id: None,
                app_id: None,
            },
        )
        .unwrap()
        .is_empty());
    }
}

//! Persist live collector samples and run SQLite maintenance.

use crate::alerts::{AlertEngine, EvaluationInput, LogSample};
use crate::collectors::redis::RedisSnapshot;
use crate::metrics::OverviewServerMetrics;
use crate::state::AppState;
use crate::storage::{run_maintenance, RawSample};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn samples_from_overview(m: &OverviewServerMetrics) -> Vec<RawSample> {
    let ts = now_ms();
    vec![
        raw(&m.server_id, "cpu.percent", ts, m.cpu_percent),
        raw(&m.server_id, "memory.percent", ts, m.memory_percent),
        raw(&m.server_id, "disk.percent", ts, m.disk_percent),
        raw(&m.server_id, "load.1", ts, m.load_1),
    ]
}

pub fn samples_from_web(
    server_id: &str,
    req_per_sec: f64,
    http_5xx_rate: f64,
    endpoint_p95: f64,
) -> Vec<RawSample> {
    let ts = now_ms();
    vec![
        raw(server_id, "web.req_per_sec", ts, req_per_sec),
        raw(server_id, "http.5xx_rate_percent", ts, http_5xx_rate),
        raw(server_id, "endpoint.p95_ms", ts, endpoint_p95),
    ]
}

pub fn samples_from_redis(server_id: &str, snap: &RedisSnapshot) -> Vec<RawSample> {
    let ts = now_ms();
    let mut out = vec![
        raw(server_id, "redis.ops_per_sec", ts, snap.derived.ops_per_sec),
        raw(server_id, "redis.hit_rate_percent", ts, snap.derived.hit_rate_percent),
        raw(
            server_id,
            "redis.memory_used_bytes",
            ts,
            snap.derived.memory_used_bytes as f64,
        ),
    ];
    if let Some(ratio) = snap.derived.memory_used_ratio {
        out.push(raw(server_id, "redis.memory_used_ratio", ts, ratio));
    }
    out
}

fn log_samples(state: &AppState, server_id: &str) -> Vec<LogSample> {
    let Some(snapshot) = state.event_hub.last_log_snapshot(server_id) else {
        return Vec::new();
    };
    let apps = state.apps_for_server(server_id);
    snapshot
        .lines
        .iter()
        .rev()
        .take(400)
        .map(|line| {
            let ids = crate::logs::trace::extract_trace_ids(&line.raw);
            let app = line
                .app_id
                .as_deref()
                .and_then(|id| apps.iter().find(|app| app.id == id))
                .map(|app| app.name.clone())
                .or(line.app_id.clone())
                .unwrap_or_default();
            LogSample {
                raw: line.raw.clone(),
                level: line.level.as_str().into(),
                message: line.message.clone(),
                app,
                source: line.source_id.clone(),
                status: line.http_status.map(|status| status as f64),
                trace_id: ids.trace_id.unwrap_or_default(),
                span_id: ids.span_id.unwrap_or_default(),
                ts_ms: line.ts_ms as i64,
            }
        })
        .collect()
}

fn raw(server_id: &str, key: &str, ts_ms: i64, value: f64) -> RawSample {
    RawSample {
        server_id: server_id.to_string(),
        metric_key: key.to_string(),
        ts_ms,
        value,
    }
}

pub fn persist_samples(state: &AppState, batch: Vec<RawSample>) {
    if batch.is_empty() {
        return;
    }
    let store = state.metrics.lock();
    if let Err(err) = crate::phase_b2::record_baselines(store.connection(), &batch) {
        eprintln!("baseline persist: {err}");
    }
    if let Err(err) = store.insert_raw_batch(&batch) {
        eprintln!("metrics persist: {}", err);
    }
}

pub fn evaluate_live_alerts(
    app: &AppHandle,
    server_id: &str,
    connected: bool,
    metrics: HashMap<String, f64>,
) {
    let state = app.state::<AppState>();
    if crate::license::require_write().is_err() { return; }
    let mut metrics = metrics;
    if let Some(apm) = state.event_hub.last_apm_snapshot(server_id) {
        for endpoint in apm.endpoints {
            metrics.insert(format!("trace.{}.p95_ms", endpoint.route), endpoint.p95_ms);
            metrics.insert(
                format!("trace.{}.error_rate", endpoint.route),
                endpoint.error_rate * 100.0,
            );
        }
    }
    let logs_available = state.event_hub.last_log_snapshot(server_id).is_some();
    let logs = log_samples(&state, server_id);
    let metrics_for_intel = metrics.clone();
    let input = EvaluationInput {
        server_id: server_id.to_string(),
        server_name: state.config.lock().get_server(server_id).map(|s| s.name.clone()).unwrap_or_else(|_| "Monitored server".into()),
        ts_ms: now_ms(),
        metrics,
        connected,
        utc_offset_secs: chrono::Local::now().offset().local_minus_utc(),
        logs,
        logs_available,
    };
    let outcome = {
        let store = state.metrics.lock();
        AlertEngine::evaluate(store.connection(), &input)
    };
    match outcome {
        Ok(outcome) => {
            for transition in outcome.transitions {
                let _ = app.emit("alert-transition", &transition);
            }
        }
        Err(err) => eprintln!("alert evaluate: {}", err),
    }
    crate::intel::on_live_sample(app, server_id, connected, &metrics_for_intel);
}

pub fn spawn_storage_maintenance(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            let state = app.state::<AppState>();
            let now = now_ms();
            {
                let store = state.metrics.lock();
                if let Err(err) = run_maintenance(&*store, now) {
                    eprintln!("storage maintenance: {}", err);
                }
            }
        }
    });
}

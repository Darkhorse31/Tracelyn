//! Teams and intelligence: extended alert rules, channels, anomalies, incidents, digests.
//!
//! ponytail: outbound jobs sit in one global queue capped at 50. A per-channel
//! queue is the upgrade if a slow webhook starts delaying the others.

pub(crate) mod checks;
pub(crate) mod cycle;
pub(crate) mod logic;
pub(crate) mod notify;
pub(crate) mod report;
pub(crate) mod store;
pub(crate) mod types;

use crate::apps::types::AppMetrics;
use crate::state::{AppState, ConnectionStatus};
use cycle::{LogObs, RestartObs, Sample};
use logic::digest_due;
use notify::{build_jobs, deliver_job, Job};
use parking_lot::Mutex;
use report::digest_message;
use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

pub use store::migrate;

fn outbox() -> &'static Mutex<VecDeque<Job>> {
    static OUTBOX: OnceLock<Mutex<VecDeque<Job>>> = OnceLock::new();
    OUTBOX.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn enqueue(jobs: Vec<Job>) {
    let mut queue = outbox().lock();
    for job in jobs {
        if queue.len() >= 50 {
            queue.pop_front();
        }
        queue.push_back(job);
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn on_live_sample(app: &AppHandle, server_id: &str, connected: bool, metrics: &HashMap<String, f64>) {
    if let Err(err) = live_sample(app, server_id, connected, metrics) {
        eprintln!("intel sample: {err}");
    }
}

fn live_sample(
    app: &AppHandle,
    server_id: &str,
    connected: bool,
    metrics: &HashMap<String, f64>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let server_name = state
        .config
        .lock()
        .get_server(server_id)
        .map(|server| server.name.clone())
        .unwrap_or_else(|_| server_id.to_string());
    let mut metrics = metrics.clone();
    if let Some(web) = state.event_hub.last_web_snapshot(server_id) {
        metrics
            .entry("web.req_per_sec".into())
            .or_insert(web.metrics.req_per_sec);
    }
    let apps = state.apps_for_server(server_id);
    fold_app_metrics(&mut metrics, &apps);
    let restarts: Vec<RestartObs> = apps
        .iter()
        .filter_map(|app_row| {
            app_row.restarts.map(|count| RestartObs {
                id: app_row.id.clone(),
                name: app_row.name.clone(),
                restarts: count as u64,
            })
        })
        .collect();
    let want_logs = {
        let db = state.metrics.lock();
        migrate(db.connection()).map_err(|e| e.to_string())?;
        store::list_rules(db.connection())
            .map_err(|e| e.to_string())?
            .iter()
            .any(|rule| rule.enabled && rule.kind == "log_pattern")
    };
    let logs = if want_logs {
        recent_logs(&state, server_id)
    } else {
        Vec::new()
    };
    let notices = {
        let db = state.metrics.lock();
        let probes = HashMap::new();
        let sample = Sample {
            server_id,
            server_name: &server_name,
            ts_ms: now_ms(),
            metrics: &metrics,
            logs: &logs,
            restarts: &restarts,
            probes: &probes,
            eval_metrics: connected,
            eval_logs: true,
            eval_anomaly: connected,
        };
        let out = cycle::run(db.connection(), &sample).map_err(|e| e.to_string())?;
        for err in &out.errors {
            eprintln!("intel rule: {err}");
        }
        out.notices
    };
    if notices.is_empty() {
        return Ok(());
    }
    let channels = channels_with_secrets(&state)?;
    for notice in notices {
        let (jobs, errors) = build_jobs(&channels, &notice.title, &notice.body);
        for err in errors {
            eprintln!("intel channel: {err}");
        }
        enqueue(jobs);
    }
    Ok(())
}

fn fold_app_metrics(metrics: &mut HashMap<String, f64>, apps: &[crate::apps::types::TracelynApp]) {
    let mut error_rate: Option<f64> = None;
    let mut p95: Option<f64> = None;
    for app_row in apps {
        let Some(AppMetrics {
            error_rate: err,
            p95_ms,
            ..
        }) = &app_row.metrics
        else {
            continue;
        };
        if let Some(value) = err {
            error_rate = Some(error_rate.map(|current| current.max(*value)).unwrap_or(*value));
        }
        if let Some(value) = p95_ms {
            p95 = Some(p95.map(|current| current.max(*value)).unwrap_or(*value));
        }
    }
    if let Some(value) = error_rate {
        metrics.entry("app.error_rate".into()).or_insert(value);
    }
    if let Some(value) = p95 {
        metrics.entry("app.p95_ms".into()).or_insert(value);
    }
}

fn recent_logs(state: &AppState, server_id: &str) -> Vec<LogObs> {
    let Some(snapshot) = state.event_hub.last_log_snapshot(server_id) else {
        return Vec::new();
    };
    snapshot
        .lines
        .iter()
        .rev()
        .take(400)
        .map(|line| LogObs {
            id: line.id.clone(),
            ts_ms: line.ts_ms as i64,
            text: line.raw.clone(),
        })
        .collect()
}

fn channels_with_secrets(state: &AppState) -> Result<Vec<types::Channel>, String> {
    let db = state.metrics.lock();
    let mut channels = store::list_channels(db.connection()).map_err(|e| e.to_string())?;
    for channel in &mut channels {
        channel.has_secret = channel.kind == "windows" || notify::has_channel_secret(&channel.id);
    }
    Ok(channels)
}

pub fn spawn_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticks: u32 = 0;
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            ticks = ticks.wrapping_add(1);
            let app = app.clone();
            let heavy = ticks % 30 == 0;
            let _ = tokio::task::spawn_blocking(move || {
                flush_outbox(&app);
                if heavy {
                    if let Err(err) = maintenance(&app) {
                        eprintln!("intel maintenance: {err}");
                    }
                }
            })
            .await;
        }
    });
}

fn flush_outbox(app: &AppHandle) {
    let jobs: Vec<Job> = outbox().lock().drain(..).collect();
    for job in jobs {
        if let Err(err) = deliver_job(&job, &mut |title, body| show_notification(app, title, body)) {
            eprintln!("intel deliver: {err}");
        }
    }
}

pub fn deliver_now(app: &AppHandle, jobs: &[Job]) -> Vec<String> {
    let mut errors = Vec::new();
    for job in jobs {
        if let Err(err) = deliver_job(job, &mut |title, body| show_notification(app, title, body)) {
            errors.push(err);
        }
    }
    errors
}

fn show_notification(app: &AppHandle, title: &str, body: &str) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|err| err.to_string())?;
    let _ = app.emit(
        "intel-notice",
        serde_json::json!({ "title": title, "body": body }),
    );
    Ok(())
}

fn maintenance(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let servers: Vec<(String, String)> = {
        let config = state.config.lock();
        config
            .list_servers()
            .iter()
            .map(|server| (server.id.clone(), server.name.clone()))
            .collect()
    };
    let now = now_ms();
    let plans = {
        let db = state.metrics.lock();
        migrate(db.connection()).map_err(|e| e.to_string())?;
        let rules = store::list_rules(db.connection()).map_err(|e| e.to_string())?;
        let mut plans = Vec::new();
        for (server_id, server_name) in &servers {
            let mut todo = Vec::new();
            for rule in rules.iter().filter(|rule| {
                rule.enabled
                    && matches!(rule.kind.as_str(), "ssl_expiry" | "uptime")
                    && rule.server_id.as_ref().map(|id| id == server_id).unwrap_or(true)
            }) {
                let runtime = store::load_state(db.connection(), &rule.id, server_id).map_err(|e| e.to_string())?;
                if now.saturating_sub(runtime.checked_at_ms) < 300_000 {
                    continue;
                }
                todo.push((
                    rule.id.clone(),
                    rule.kind.clone(),
                    rule.params.host.clone(),
                    rule.params.port,
                ));
            }
            if !todo.is_empty() {
                plans.push((server_id.clone(), server_name.clone(), todo));
            }
        }
        plans
    };
    for (server_id, server_name, todo) in plans {
        let mut probes = HashMap::new();
        for (id, kind, host, port) in todo {
            let result = if kind == "ssl_expiry" {
                checks::probe_ssl_days(&host, port, now)
            } else {
                checks::probe_tcp(&host, port)
            };
            probes.insert(id, result);
        }
        let restarts = restart_obs(&state, &server_id);
        let logs = recent_logs(&state, &server_id);
        let metrics = HashMap::new();
        let db = state.metrics.lock();
        let sample = Sample {
            server_id: &server_id,
            server_name: &server_name,
            ts_ms: now,
            metrics: &metrics,
            logs: &logs,
            restarts: &restarts,
            probes: &probes,
            eval_metrics: false,
            eval_logs: true,
            eval_anomaly: false,
        };
        match cycle::run(db.connection(), &sample) {
            Ok(out) => {
                drop(db);
                if !out.notices.is_empty() {
                    let channels = channels_with_secrets(&state)?;
                    for notice in out.notices {
                        let (jobs, _) = build_jobs(&channels, &notice.title, &notice.body);
                        enqueue(jobs);
                    }
                }
            }
            Err(err) => eprintln!("intel probe cycle: {err}"),
        }
    }
    send_due_digests(app, &state, now)?;
    let db = state.metrics.lock();
    store::prune(db.connection()).map_err(|e| e.to_string())?;
    Ok(())
}

fn restart_obs(state: &AppState, server_id: &str) -> Vec<RestartObs> {
    state
        .apps_for_server(server_id)
        .into_iter()
        .filter_map(|app_row| {
            app_row.restarts.map(|count| RestartObs {
                id: app_row.id,
                name: app_row.name,
                restarts: count as u64,
            })
        })
        .collect()
}

fn send_due_digests(app: &AppHandle, state: &AppState, now: i64) -> Result<(), String> {
    let due = {
        let db = state.metrics.lock();
        migrate(db.connection()).map_err(|e| e.to_string())?;
        store::list_digests(db.connection())
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|digest| {
                digest_due(
                    &logic::DueDigest {
                        cadence: digest.cadence.clone(),
                        hour_utc: digest.hour_utc,
                        weekday: digest.weekday,
                        enabled: digest.enabled,
                        last_sent_ms: digest.last_sent_ms,
                        last_attempt_ms: digest.last_attempt_ms,
                    },
                    now,
                )
            })
            .collect::<Vec<_>>()
    };
    for mut digest in due {
        digest.last_attempt_ms = Some(now);
        let (subject, body, channel) = {
            let db = state.metrics.lock();
            let since = if digest.cadence == "weekly" {
                now - 7 * 86_400_000
            } else {
                now - 86_400_000
            };
            let incidents = store::incidents_since(db.connection(), since).map_err(|e| e.to_string())?;
            let (_, anomaly_count) = store::incident_counts(db.connection(), since).map_err(|e| e.to_string())?;
            let (subject, body) = digest_message(&digest, &incidents, anomaly_count, since);
            let channel = store::list_channels(db.connection())
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|item| item.id == digest.channel_id);
            store::upsert_digest(db.connection(), &digest).map_err(|e| e.to_string())?;
            (subject, body, channel)
        };
        let Some(channel) = channel else {
            digest.last_error = Some("channel was deleted".into());
            let db = state.metrics.lock();
            store::upsert_digest(db.connection(), &digest).map_err(|e| e.to_string())?;
            continue;
        };
        let (jobs, mut errors) = build_jobs(&[channel], &subject, &body);
        errors.extend(deliver_now(app, &jobs));
        if errors.is_empty() {
            digest.last_sent_ms = Some(now);
            digest.last_error = None;
        } else {
            digest.last_error = Some(errors.join(". "));
        }
        let db = state.metrics.lock();
        store::upsert_digest(db.connection(), &digest).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn run_for_server(
    state: &AppState,
    server_id: &str,
    probes: &HashMap<String, Result<f64, String>>,
) -> Result<cycle::CycleOut, String> {
    let server_name = state
        .config
        .lock()
        .get_server(server_id)
        .map(|server| server.name.clone())
        .unwrap_or_else(|_| server_id.to_string());
    let connected = matches!(
        state.runtime(server_id).status,
        ConnectionStatus::Connected | ConnectionStatus::Degraded
    );
    let apps = state.apps_for_server(server_id);
    let mut metrics = HashMap::new();
    fold_app_metrics(&mut metrics, &apps);
    if let Some(web) = state.event_hub.last_web_snapshot(server_id) {
        metrics.insert("web.req_per_sec".into(), web.metrics.req_per_sec);
    }
    let restarts = restart_obs(state, server_id);
    let logs = recent_logs(state, server_id);
    let db = state.metrics.lock();
    migrate(db.connection()).map_err(|e| e.to_string())?;
    let sample = Sample {
        server_id,
        server_name: &server_name,
        ts_ms: now_ms(),
        metrics: &metrics,
        logs: &logs,
        restarts: &restarts,
        probes,
        eval_metrics: connected || !metrics.is_empty(),
        eval_logs: true,
        eval_anomaly: true,
    };
    cycle::run(db.connection(), &sample).map_err(|e| e.to_string())
}

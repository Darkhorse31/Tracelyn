//! Teams and intelligence: extended alert rules, channels, anomalies, incidents, digests.
//!
//! ponytail: outbound jobs sit in one global queue capped at 50. A per-channel
//! queue is the upgrade if a slow webhook starts delaying the others.

pub(crate) mod checks;
pub(crate) mod cycle;
pub(crate) mod incidents;
pub(crate) mod logic;
pub(crate) mod notify;
pub(crate) mod policy;
pub(crate) mod report;
pub(crate) mod store;
pub(crate) mod timeline;
pub(crate) mod types;

use crate::apps::types::{AppKind, AppMetrics, AppStatus, TracelynApp};
use crate::state::{AppState, ConnectionStatus};
use cycle::{AppSignal, LogObs, RestartObs, Sample};
use logic::digest_due;
use notify::{build_jobs, deliver_job, Job};
use parking_lot::Mutex;
use report::digest_message;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Listener, Manager};
use tauri_plugin_notification::NotificationExt;

pub use store::migrate;
static PRIVATE_NOTICES: AtomicBool = AtomicBool::new(true);

fn queue_notice(app: &AppHandle, channels: &[types::Channel], notice: &cycle::Notice) {
    let _ = app.emit(
        "intel-notice",
        serde_json::json!({ "title": notice.title, "body": notice.body, "path": notice.path }),
    );
    let (mut jobs, _) = build_jobs(channels, &notice.title, &notice.body, &notice.path);
    // One desktop notification regardless of the number of configured channels.
    jobs.retain(|job| !matches!(job, Job::Windows { .. }));
    jobs.push(Job::Windows {
        title: notice.title.clone(),
        body: notice.body.clone(),
        path: notice.path.clone(),
    });
    enqueue(jobs);
}

fn outbox() -> &'static Mutex<VecDeque<Job>> {
    static OUTBOX: OnceLock<Mutex<VecDeque<Job>>> = OnceLock::new();
    OUTBOX.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn enqueue(jobs: Vec<Job>) {
    let mut queue = outbox().lock();
    for job in jobs {
        queue.push_back(job);
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn on_live_sample(
    app: &AppHandle,
    server_id: &str,
    connected: bool,
    metrics: &HashMap<String, f64>,
) {
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
    if !connected {
        let host = state
            .config
            .lock()
            .get_server(server_id)
            .map(|server| server.host.clone())
            .unwrap_or_default();
        if let Ok(up) = checks::probe_ping(&host) {
            metrics.insert("server.ping_up".into(), if up { 1.0 } else { 0.0 });
        }
    }
    if let Some(web) = state.event_hub.last_web_snapshot(server_id) {
        metrics
            .entry("web.req_per_sec".into())
            .or_insert(web.metrics.req_per_sec);
    }
    let apps = state.apps_for_server(server_id);
    metrics.insert("server.connected".into(), if connected { 1.0 } else { 0.0 });
    fold_app_metrics(&mut metrics, &apps);
    enrich_infra(app, server_id, &mut metrics);
    let signals = app_signals(&apps);
    let services = service_flags(&apps);
    let (fail_count, login_lines) = auth_signals(app, server_id);
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
            .any(|rule| rule.enabled && matches!(rule.kind.as_str(), "log_pattern" | "new_error"))
    };
    let logs = if want_logs || connected {
        recent_logs(&state, server_id)
    } else {
        Vec::new()
    };
    let notices = {
        let db = state.metrics.lock();
        migrate(db.connection()).map_err(|e| e.to_string())?;
        store::seed_defaults(db.connection(), now_ms()).map_err(|e| e.to_string())?;
        let probes = HashMap::new();
        let sample = Sample {
            server_id,
            server_name: &server_name,
            ts_ms: now_ms(),
            metrics: &metrics,
            logs: &logs,
            restarts: &restarts,
            apps: &signals,
            services: &services,
            fail_count,
            login_lines: login_lines.as_deref(),
            utc_offset_secs: local_offset_secs(),
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
    refresh_tray(app);
    if notices.is_empty() {
        return Ok(());
    }
    let channels = channels_with_secrets(&state)?;
    for notice in notices {
        queue_notice(app, &channels, &notice);
    }
    Ok(())
}

fn refresh_tray(app: &AppHandle) {
    let state = app.state::<AppState>();
    let firing = {
        let db = state.metrics.lock();
        let intel: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM intel_rule_state WHERE firing = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let host: i64 = db
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM alert_rule_state WHERE firing = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        intel.saturating_add(host)
    };
    let text = if firing == 0 {
        "Tracelyn · all clear".to_string()
    } else if firing == 1 {
        "Tracelyn · 1 alert firing".to_string()
    } else {
        format!("Tracelyn · {firing} alerts firing")
    };
    if let Some(tray) = app.tray_by_id("status") {
        let _ = tray.set_tooltip(Some(text));
    }
}

fn local_offset_secs() -> i32 {
    chrono::Local::now().offset().local_minus_utc()
}

fn app_signals(apps: &[TracelynApp]) -> Vec<AppSignal> {
    apps.iter()
        .take(200)
        .map(|app| AppSignal {
            id: app.id.clone(),
            name: app.name.clone(),
            crashed: app_crashed(app),
            restarts: app.restarts.unwrap_or(0) as u64,
            version: app_version(app),
        })
        .collect()
}

fn app_crashed(app: &TracelynApp) -> bool {
    let oom = app.tags.iter().any(|tag| tag.eq_ignore_ascii_case("oom"));
    match app.kind {
        AppKind::Pm2 | AppKind::Docker => {
            oom || matches!(app.status, AppStatus::Stopped | AppStatus::Errored)
        }
        AppKind::Systemd => oom || matches!(app.status, AppStatus::Errored),
        _ => oom || matches!(app.status, AppStatus::Errored),
    }
}

fn app_version(app: &TracelynApp) -> String {
    for prefix in ["git:", "image:"] {
        if let Some(tag) = app.tags.iter().find_map(|tag| tag.strip_prefix(prefix)) {
            if !tag.is_empty() {
                return format!("{prefix}{tag}");
            }
        }
    }
    match (&app.cwd, &app.interpreter) {
        (Some(cwd), Some(bin)) if !cwd.is_empty() || !bin.is_empty() => format!("{cwd}|{bin}"),
        (Some(cwd), None) if !cwd.is_empty() => cwd.clone(),
        (None, Some(bin)) if !bin.is_empty() => bin.clone(),
        _ => String::new(),
    }
}

fn service_flags(apps: &[TracelynApp]) -> Vec<(String, bool)> {
    let mut flags: Vec<(String, bool)> = Vec::new();
    for app in apps {
        let name = service_name(app);
        let Some(name) = name else { continue };
        let running = app.status == AppStatus::Online;
        if let Some(slot) = flags.iter_mut().find(|(existing, _)| existing == &name) {
            slot.1 |= running;
        } else {
            flags.push((name, running));
        }
    }
    flags
}

fn service_name(app: &TracelynApp) -> Option<String> {
    match app.kind {
        AppKind::Nginx => Some("nginx".into()),
        AppKind::Apache => Some("apache".into()),
        _ => {
            let name = app.name.to_ascii_lowercase();
            if name.contains("mysql") || name.contains("mariadb") {
                Some("mysql".into())
            } else if name.contains("postgres") {
                Some("postgres".into())
            } else if name.contains("redis") {
                Some("redis".into())
            } else {
                None
            }
        }
    }
}

fn enrich_infra(app: &AppHandle, server_id: &str, metrics: &mut HashMap<String, f64>) {
    let infra = app.state::<crate::infra::InfraState>();
    if let Some(days) = infra.minimum_number(server_id, "capacity", "daysUntilFull") {
        metrics.insert("disk.forecast_days".into(), days);
    }
    if let Some(up) = infra.uptime_ok(server_id) {
        metrics.insert("health.up".into(), if up { 1.0 } else { 0.0 });
    }
    for collector in ["certificates", "certificates_pc"] {
        if let Some(days) = infra.minimum_number(server_id, collector, "daysRemaining") {
            let previous = metrics.get("ssl.days_remaining").copied().unwrap_or(days);
            metrics.insert("ssl.days_remaining".into(), previous.min(days));
        }
    }
}

fn auth_signals(app: &AppHandle, server_id: &str) -> (Option<u64>, Option<Vec<String>>) {
    let infra = app.state::<crate::infra::InfraState>();
    let fails = if infra.has_collector(server_id, "ssh_security") {
        Some(
            infra
                .maximum_number(server_id, "ssh_security", "attempts")
                .unwrap_or(0.0) as u64,
        )
    } else {
        None
    };
    let lines = if infra.has_collector(server_id, "last_logins") {
        Some(infra.field_strings(server_id, "last_logins", "session"))
    } else {
        None
    };
    (fails, lines)
}

fn fold_app_metrics(metrics: &mut HashMap<String, f64>, apps: &[TracelynApp]) {
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
            error_rate = Some(
                error_rate
                    .map(|current| current.max(*value))
                    .unwrap_or(*value),
            );
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
    app.listen("privacy-mode", |event| {
        PRIVATE_NOTICES.store(event.payload() != "false", Ordering::SeqCst);
    });
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
        if let Err(err) = deliver_job(&job, &mut |title, body, path| {
            show_notification(app, title, body, path)
        }) {
            eprintln!("intel deliver: {err}");
        }
    }
}

pub fn deliver_now(app: &AppHandle, jobs: &[Job]) -> Vec<String> {
    let mut errors = Vec::new();
    for job in jobs {
        if let Err(err) = deliver_job(job, &mut |title, body, path| {
            show_notification(app, title, body, path)
        }) {
            errors.push(err);
        }
    }
    errors
}

fn show_notification(app: &AppHandle, title: &str, body: &str, path: &str) -> Result<(), String> {
    let private = PRIVATE_NOTICES.load(Ordering::SeqCst);
    let (title, body) = incidents::notification_text(private, title, body);
    app.notification()
        .builder()
        .title(&title)
        .body(&body)
        .extra("path", path)
        .show()
        .map_err(|err| err.to_string())?;
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
                    && matches!(rule.kind.as_str(), "ssl_expiry" | "uptime" | "app_down")
                    && (rule.kind != "app_down" || !rule.params.host.is_empty())
                    && rule
                        .server_id
                        .as_ref()
                        .map(|id| id == server_id)
                        .unwrap_or(true)
            }) {
                let runtime = store::load_state(db.connection(), &rule.id, server_id)
                    .map_err(|e| e.to_string())?;
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
        let no_apps = Vec::new();
        let no_services = Vec::new();
        let db = state.metrics.lock();
        let sample = Sample {
            server_id: &server_id,
            server_name: &server_name,
            ts_ms: now,
            metrics: &metrics,
            logs: &logs,
            restarts: &restarts,
            apps: &no_apps,
            services: &no_services,
            fail_count: None,
            login_lines: None,
            utc_offset_secs: local_offset_secs(),
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
                        queue_notice(app, &channels, &notice);
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
            let incidents =
                store::incidents_since(db.connection(), since).map_err(|e| e.to_string())?;
            let (_, anomaly_count) =
                store::incident_counts(db.connection(), since).map_err(|e| e.to_string())?;
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
        let (jobs, mut errors) = build_jobs(&[channel], &subject, &body, "/alerts");
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
    metrics.insert("server.connected".into(), if connected { 1.0 } else { 0.0 });
    fold_app_metrics(&mut metrics, &apps);
    if let Some(web) = state.event_hub.last_web_snapshot(server_id) {
        metrics.insert("web.req_per_sec".into(), web.metrics.req_per_sec);
    }
    let signals = app_signals(&apps);
    let services = service_flags(&apps);
    let restarts = restart_obs(state, server_id);
    let logs = recent_logs(state, server_id);
    let db = state.metrics.lock();
    migrate(db.connection()).map_err(|e| e.to_string())?;
    store::seed_defaults(db.connection(), now_ms()).map_err(|e| e.to_string())?;
    let sample = Sample {
        server_id,
        server_name: &server_name,
        ts_ms: now_ms(),
        metrics: &metrics,
        logs: &logs,
        restarts: &restarts,
        apps: &signals,
        services: &services,
        fail_count: None,
        login_lines: None,
        utc_offset_secs: local_offset_secs(),
        probes,
        eval_metrics: connected || !metrics.is_empty(),
        eval_logs: true,
        eval_anomaly: true,
    };
    cycle::run(db.connection(), &sample).map_err(|e| e.to_string())
}

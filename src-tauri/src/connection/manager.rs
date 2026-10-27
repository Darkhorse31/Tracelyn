use crate::apps::types::AppMetrics;
use crate::apps::{discover_apps, load_manual_apps};
use crate::ssh::SshSession;
use crate::collectors::redis::{CollectError, RedisCollector};
use crate::collectors::redis::RedisCollectorState;
use crate::collectors::system::SystemCollectorState;
use crate::config::RedisTargetConfig;
use crate::connection::redis_probe::{probe_redis_endpoints, RedisEndpoint};
use crate::diagnostics::build_diagnostics;
use crate::events::{LogFacets, LogStreamSnapshot, SourceStatus};
use crate::insight::InsightPoller;
use crate::logs::follow::{status_from_message, TailHub};
use crate::logs::LogRingBuffer;
use crate::config::ServerRecord;
use crate::connection::ssh_runner::SessionRunner;
use crate::connection::web_pipeline::WebPipeline;
use crate::events::EventHub;
use crate::metrics_ingest::{
    evaluate_live_alerts, persist_samples, samples_from_overview, samples_from_redis,
    samples_from_web,
};
use crate::ssh::{connect_persistent, run_command};
use crate::state::{AppState, ConnectionStatus, ServerRuntime};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{async_runtime::JoinHandle, AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

const OFFLINE_AFTER: Duration = Duration::from_secs(60);
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(60);

pub struct ConnectionManager {
    host_keys: Arc<Mutex<crate::ssh::HostKeyStore>>,
    event_hub: Arc<EventHub>,
    tasks: Mutex<HashMap<String, JoinHandle<()>>>,
}

impl ConnectionManager {
    pub fn new(host_keys: Arc<Mutex<crate::ssh::HostKeyStore>>, event_hub: Arc<EventHub>) -> Self {
        Self {
            host_keys,
            event_hub,
            tasks: Mutex::new(HashMap::new()),
        }
    }

    pub async fn connect(&self, app: AppHandle, server: ServerRecord) -> Result<(), String> {
        self.disconnect(&server.id).await;

        let host_keys = self.host_keys.clone();
        let event_hub = self.event_hub.clone();
        let server_id = server.id.clone();

        {
            let state = app.state::<AppState>();
            state.set_runtime(
                &server_id,
                ServerRuntime {
                    status: ConnectionStatus::Connecting,
                    os_info: None,
                    hostname: None,
                    last_error: None,
                },
            );
            emit_statuses(&app);
        }

        let app_clone = app.clone();
        let handle = tauri::async_runtime::spawn(async move {
            run_server_loop(app_clone, host_keys, event_hub, server).await;
        });

        self.tasks.lock().await.insert(server_id, handle);
        Ok(())
    }

    pub async fn disconnect(&self, server_id: &str) {
        if let Some(handle) = self.tasks.lock().await.remove(server_id) {
            handle.abort();
        }
    }
}

fn emit_statuses(app: &AppHandle) {
    let state = app.state::<AppState>();
    let statuses = state.all_statuses();
    let _ = app.emit("server-statuses", &statuses);
}

async fn run_server_loop(
    app: AppHandle,
    host_keys: Arc<Mutex<crate::ssh::HostKeyStore>>,
    event_hub: Arc<EventHub>,
    server: ServerRecord,
) {
    let server_id = server.id.clone();
    let mut backoff = BACKOFF_MIN;
    let mut disconnected_since: Option<Instant> = None;

    loop {
        match connect_persistent(&server, host_keys.clone()).await {
            Ok(session) => {
                backoff = BACKOFF_MIN;
                disconnected_since = None;

                let os_info = run_command(&session, "cat /etc/os-release 2>/dev/null || uname -a")
                    .await
                    .map(|s| s.trim().to_string())
                    .ok();
                let hostname = run_command(&session, "hostname -f 2>/dev/null || hostname")
                    .await
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());

                {
                    let state = app.state::<AppState>();
                    state.set_runtime(
                        &server_id,
                        ServerRuntime {
                            status: ConnectionStatus::Connected,
                            os_info,
                            hostname,
                            last_error: None,
                        },
                    );
                }
                emit_statuses(&app);

                let result = tokio::select! {
                    result = collector_loop(&app, &session, &server_id, &event_hub) => result,
                    _ = crate::infra::remote::run(&app, &session, &server_id) => Err("Infrastructure polling stopped".into()),
                };
                if let Err(err) = result {
                    let state = app.state::<AppState>();
                    let prev = state.runtime(&server_id);
                    state.set_runtime(
                        &server_id,
                        ServerRuntime {
                            status: ConnectionStatus::Degraded,
                            os_info: prev.os_info,
                            hostname: prev.hostname,
                            last_error: Some(err),
                        },
                    );
                    emit_statuses(&app);
                }

                let _ = session
                    .disconnect(russh::Disconnect::ByApplication, "", "English")
                    .await;
            }
            Err(err) => {
                let held = if let crate::ssh::SshError::NeedsTrust(prompt) = &err {
                    let _ = app.emit("host-key-prompt", prompt);
                    if prompt.changed {
                        "SSH host key changed. The connection stays blocked until you choose Trust or Cancel.".into()
                    } else {
                        "Confirm the SSH host key fingerprint before this server can connect.".into()
                    }
                } else {
                    err.to_string()
                };
                let msg = held;
                if disconnected_since.is_none() {
                    disconnected_since = Some(Instant::now());
                }
                let status = if disconnected_since.unwrap().elapsed() >= OFFLINE_AFTER {
                    ConnectionStatus::Offline
                } else {
                    ConnectionStatus::Degraded
                };

                let state = app.state::<AppState>();
                let prev = state.runtime(&server_id);
                state.set_runtime(
                    &server_id,
                    ServerRuntime {
                        status,
                        os_info: prev.os_info,
                        hostname: prev.hostname,
                        last_error: Some(msg),
                    },
                );
                emit_statuses(&app);

                let mut metrics = HashMap::new();
                metrics.insert("server.connected".into(), 0.0);
                evaluate_live_alerts(&app, &server_id, false, metrics);
            }
        }

        if disconnected_since.is_none() {
            disconnected_since = Some(Instant::now());
        } else {
            let state = app.state::<AppState>();
            let prev = state.runtime(&server_id);
            if prev.status == ConnectionStatus::Connected {
                let since = disconnected_since.unwrap();
                let status = if since.elapsed() >= OFFLINE_AFTER {
                    ConnectionStatus::Offline
                } else {
                    ConnectionStatus::Degraded
                };
                state.set_runtime(
                    &server_id,
                    ServerRuntime {
                        status,
                        os_info: prev.os_info,
                        hostname: prev.hostname,
                        last_error: prev.last_error,
                    },
                );
                emit_statuses(&app);
            }
        }

        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

async fn collector_loop(
    app: &AppHandle,
    session: &SshSession,
    server_id: &str,
    event_hub: &Arc<EventHub>,
) -> Result<(), String> {
    let mut state = SystemCollectorState::new();
    let route_rules = app.state::<AppState>().config.lock().route_rules(server_id);
    let mut web = WebPipeline::setup(session, route_rules).await;
    let mut redis_endpoints = probe_redis_endpoints(session).await;
    let saved_target = app.state::<AppState>().config.lock().redis_target(server_id);
    if redis_endpoints.is_empty() && target_configured(&saved_target) {
        redis_endpoints.push(endpoint_from_target(&saved_target));
    }
    let mut redis_collector = if redis_endpoints.is_empty() {
        None
    } else {
        let chosen = choose_endpoint(&redis_endpoints, &saved_target);
        let mut collector = RedisCollector::new(SessionRunner::new(session), server_id);
        collector.set_dial(chosen.to_dial());
        Some(collector)
    };

    let mut fast = tokio::time::interval(Duration::from_secs(2));
    let mut slow = tokio::time::interval(Duration::from_secs(5));
    let mut services_iv = tokio::time::interval(Duration::from_secs(15));
    let mut web_iv = tokio::time::interval(Duration::from_secs(1));
    let mut redis_iv = tokio::time::interval(Duration::from_secs(10));
    let mut discovery_iv = tokio::time::interval(Duration::from_secs(60));
    let mut log_buffer = LogRingBuffer::new();
    let mut tails = TailHub::new();
    let mut insight = InsightPoller::default();
    let mut redis_phase = if redis_collector.is_some() {
        "loading".to_string()
    } else {
        "not_installed".to_string()
    };
    let mut redis_failures: u32 = 0;
    let mut redis_retry_at = Instant::now();
    let mut collector_error: Option<String> = None;
    let mut collector_name: Option<String> = None;
    let mut collector_failures: u32 = 0;
    fast.tick().await;
    slow.tick().await;
    services_iv.tick().await;
    web_iv.tick().await;
    redis_iv.tick().await;
    discovery_iv.tick().await;

    if run_app_discovery(app, session, server_id, event_hub).await {
        tails.mark_stale();
    }
    insight.detect(session, app, server_id).await;
    push_redis_phase(
        event_hub,
        server_id,
        &redis_phase,
        None,
        &redis_endpoints,
    );

    loop {
        if let Some(prompt) = session.take_host_key_prompt().await {
            let _ = app.emit("host-key-prompt", &prompt);
        }
        web.drain_log_lines();
        ingest_access_lines(app, server_id, &mut web, &mut log_buffer);
        tails.drain(&mut log_buffer);
        {
            let state_app = app.state::<AppState>();
            let filter = state_app.log_filter(server_id);
            let apps = state_app.apps_for_server(server_id);
            drop(state_app);
            let app_filter = if filter.app_ids.is_empty() {
                None
            } else {
                Some(filter.app_ids.as_slice())
            };
            let source_filter = if filter.sources.is_empty() {
                None
            } else {
                Some(filter.sources.as_slice())
            };
            tails
                .sync(
                    session,
                    &apps,
                    app_filter,
                    source_filter,
                    filter.generation,
                )
                .await;
        }

        if app.state::<AppState>().take_rediscovery_requested(server_id)
            && run_app_discovery(app, session, server_id, event_hub).await
        {
            tails.mark_stale();
        }

        if insight.due() {
            insight.poll(session, app, server_id, &mut log_buffer).await;
        }
        let insight_diag = insight.diagnostics();
        let mut detect_now = false;

        tokio::select! {
            _ = discovery_iv.tick() => {
                detect_now = true;
                if run_app_discovery(app, session, server_id, event_hub).await {
                    tails.mark_stale();
                }
                redis_endpoints = probe_redis_endpoints(session).await;
                let saved = app.state::<AppState>().config.lock().redis_target(server_id);
                if redis_endpoints.is_empty() && target_configured(&saved) {
                    redis_endpoints.push(endpoint_from_target(&saved));
                }
                if redis_collector.is_none() && !redis_endpoints.is_empty() {
                    let chosen = choose_endpoint(&redis_endpoints, &saved);
                    let mut collector = RedisCollector::new(SessionRunner::new(session), server_id);
                    collector.set_dial(chosen.to_dial());
                    redis_collector = Some(collector);
                    redis_phase = "loading".into();
                }
            }
            _ = fast.tick() => {
                match state.poll_fast(session).await {
                    Ok(()) => {
                        collector_failures = 0;
                        collector_error = None;
                        collector_name = None;
                    }
                    Err(err) => {
                        collector_failures = collector_failures.saturating_add(1);
                        collector_error = Some(err);
                        collector_name = Some("system".into());
                    }
                }
                let overview = state.overview(server_id);
                event_hub.push_overview(overview.clone());
                event_hub.push_server_detail(state.snapshot(
                    server_id,
                    collector_error.clone(),
                    collector_name.clone(),
                    collector_failures,
                ));

                let state_app = app.state::<AppState>();
                persist_samples(&state_app, samples_from_overview(&overview));

                let mut alert_metrics = HashMap::new();
                alert_metrics.insert("cpu.percent".into(), overview.cpu_percent);
                alert_metrics.insert("memory.percent".into(), overview.memory_percent);
                alert_metrics.insert("disk.percent".into(), overview.disk_percent);
                alert_metrics.insert("server.connected".into(), 1.0);
                alert_metrics.insert("http.5xx_rate_percent".into(), web.http_5xx_rate_percent());
                alert_metrics.insert("endpoint.p95_ms".into(), web.max_endpoint_p95_ms());
                evaluate_live_alerts(app, server_id, true, alert_metrics);
            }
            _ = slow.tick() => {
                let _ = state.poll_processes(session).await;
                let _ = state.poll_ss(session).await;
                event_hub.push_server_detail(state.snapshot(
                    server_id,
                    collector_error.clone(),
                    collector_name.clone(),
                    collector_failures,
                ));
            }
            _ = services_iv.tick() => {
                let _ = state.poll_services(session).await;
                event_hub.push_server_detail(state.snapshot(
                    server_id,
                    collector_error.clone(),
                    collector_name.clone(),
                    collector_failures,
                ));
            }
            _ = web_iv.tick() => {
                let rules = app.state::<AppState>().config.lock().route_rules(server_id);
                web.set_route_rules(rules);
                let path_metrics = web.path_metrics();
                let (web_event, apm_event) = web.build_events(server_id);
                let req_per_sec = web_event.metrics.req_per_sec;
                event_hub.push_web_traffic(web_event);
                event_hub.push_apm(apm_event);
                publish_logs(app, server_id, event_hub, &mut log_buffer, &web, &tails);
                let state_app = app.state::<AppState>();
                if !path_metrics.is_empty() {
                    let mut apps = state_app.apps_for_server(server_id);
                    let mut changed = false;
                    for app_row in &mut apps {
                        for src in &app_row.log_sources {
                            let Some(path) = &src.path else { continue };
                            let Some((rps, err, p95)) = path_metrics.get(path) else { continue };
                            app_row.metrics = Some(AppMetrics {
                                req_per_sec: Some(*rps),
                                error_rate: Some(*err),
                                p95_ms: *p95,
                            });
                            changed = true;
                        }
                    }
                    if changed {
                        state_app.set_apps_for_server(server_id, apps.clone());
                        event_hub.push_apps(server_id, apps);
                    }
                }
                let apps = state_app.apps_for_server(server_id);
                let mut diag = build_diagnostics(
                    &apps,
                    web.web_detected(),
                    web.logs_readable(),
                    web.unreadable_logs(),
                    &redis_phase,
                    tails.statuses(),
                );
                diag.extend(insight_diag);
                state_app.set_diagnostics(server_id, diag);
                persist_samples(
                    &state_app,
                    samples_from_web(
                        server_id,
                        req_per_sec,
                        web.http_5xx_rate_percent(),
                        web.max_endpoint_p95_ms(),
                    ),
                );
            }
            _ = redis_iv.tick() => {
                if redis_collector.is_none() {
                    redis_phase = "not_installed".into();
                    push_redis_phase(event_hub, server_id, &redis_phase, None, &redis_endpoints);
                } else if Instant::now() < redis_retry_at {
                    let waiting = event_hub
                        .last_redis_snapshot(server_id)
                        .and_then(|v| {
                            v.get("collectorState")
                                .and_then(|s| s.get("message"))
                                .and_then(|m| m.as_str())
                                .map(str::to_string)
                        })
                        .filter(|m| !m.is_empty())
                        .unwrap_or_else(|| "Redis will retry shortly.".into());
                    push_redis_phase(
                        event_hub,
                        server_id,
                        &redis_phase,
                        Some(waiting),
                        &redis_endpoints,
                    );
                } else if let Some(collector) = redis_collector.as_mut() {
                    let saved = app.state::<AppState>().config.lock().redis_target(server_id);
                    if target_configured(&saved) {
                        collector.set_dial(endpoint_from_target(&saved).to_dial());
                    }
                    match collector.poll_over_ssh(session).await {
                        Ok(mut snap) => {
                            redis_failures = 0;
                            redis_phase = "live".into();
                            snap.collector_state = Some(RedisCollectorState {
                                phase: redis_phase.clone(),
                                message: None,
                            });
                            if let Ok(mut json) = serde_json::to_value(&snap) {
                                if let Some(obj) = json.as_object_mut() {
                                    obj.insert(
                                        "instances".into(),
                                        serde_json::to_value(&redis_endpoints).unwrap_or(serde_json::json!([])),
                                    );
                                }
                                event_hub.push_redis(server_id, json);
                            }
                            let state_app = app.state::<AppState>();
                            persist_samples(&state_app, samples_from_redis(server_id, &snap));
                            let mut alert_metrics = HashMap::new();
                            alert_metrics.insert("redis.up".into(), 1.0);
                            if let Some(ratio) = snap.derived.memory_used_ratio {
                                alert_metrics.insert("redis.memory_used_ratio".into(), ratio);
                                let percent = if ratio <= 1.5 { ratio * 100.0 } else { ratio };
                                alert_metrics.insert("redis.memory_used_percent".into(), percent);
                            }
                            evaluate_live_alerts(app, server_id, true, alert_metrics);
                        }
                        Err(err) => {
                            redis_failures = redis_failures.saturating_add(1);
                            let delay = Duration::from_secs((1u64 << redis_failures.min(6)).min(60));
                            redis_retry_at = Instant::now() + delay;
                            let (phase, message) = match err {
                                CollectError::Access { phase, message } => (phase, Some(message)),
                                CollectError::Remote(msg) => (classify_redis_error(&msg), Some(msg)),
                                CollectError::Secret(_) => {
                                    ("auth_required".into(), Some("Redis needs a password.".into()))
                                }
                                CollectError::Parse(msg) => ("error".into(), Some(msg)),
                            };
                            redis_phase = phase;
                            push_redis_phase(event_hub, server_id, &redis_phase, message, &redis_endpoints);
                            let mut alert_metrics = HashMap::new();
                            alert_metrics.insert("redis.up".into(), 0.0);
                            evaluate_live_alerts(app, server_id, true, alert_metrics);
                        }
                    }
                }
            }
        }
        if detect_now {
            insight.detect(session, app, server_id).await;
        }
    }
}

async fn run_app_discovery(
    app: &AppHandle,
    session: &SshSession,
    server_id: &str,
    event_hub: &Arc<EventHub>,
) -> bool {
    let state_app = app.state::<AppState>();
    let manual = {
        let store = state_app.metrics.lock();
        load_manual_apps(store.connection(), server_id).unwrap_or_default()
    };
    if let Ok(apps) = discover_apps(session, server_id, &manual).await {
        state_app.set_apps_for_server(server_id, apps.clone());
        event_hub.push_apps(server_id, apps);
        true
    } else {
        false
    }
}

fn ingest_access_lines(
    app: &AppHandle,
    server_id: &str,
    web: &mut WebPipeline,
    buffer: &mut LogRingBuffer,
) {
    let apps = app.state::<AppState>().apps_for_server(server_id);
    for (path, line) in web.take_recent_access() {
        let matched = apps.iter().find_map(|row| {
            row.log_sources
                .iter()
                .find(|src| src.path.as_deref() == Some(path.as_str()))
                .map(|src| (row.id.clone(), src.id.clone()))
        });
        let (app_id, source_id) = match matched {
            Some((app_id, source_id)) => (Some(app_id), source_id),
            None => (None, format!("access:{path}")),
        };
        buffer.push(&source_id, app_id.as_deref(), &line);
    }
}

fn publish_logs(
    app: &AppHandle,
    server_id: &str,
    event_hub: &Arc<EventHub>,
    buffer: &mut LogRingBuffer,
    web: &WebPipeline,
    tails: &TailHub,
) {
    let state_app = app.state::<AppState>();
    let (level, app_id) = buffer.facets();
    let mut source_status = tails.statuses().clone();
    let web_ok = web.logs_readable();
    let _tracked_logs = web.access_log_paths().len();
    source_status.insert(
        "web-access".into(),
        SourceStatus {
            ok: web_ok,
            reason: if web_ok {
                None
            } else if web.web_detected() {
                Some("access logs not readable".into())
            } else {
                Some("no web server detected".into())
            },
            fix_hint: Some(
                "Add the SSH user to the adm group, or set a readable log path on the application."
                    .into(),
            ),
        },
    );
    for path in web.unreadable_logs() {
        source_status
            .entry(format!("access:{path}"))
            .or_insert_with(|| status_from_message("Permission denied"));
    }
    let facets = LogFacets { level, app_id };
    let paused = state_app.log_paused(server_id);
    state_app.set_log_window(
        server_id,
        LogStreamSnapshot {
            lines: buffer.recent(2000),
            incremental: false,
            paused,
            source_status: source_status.clone(),
            facets: facets.clone(),
        },
    );
    event_hub.push_log_stream(
        server_id,
        LogStreamSnapshot {
            lines: buffer.drain_new(2000),
            incremental: true,
            paused,
            source_status,
            facets,
        },
    );
}

fn target_configured(target: &RedisTargetConfig) -> bool {
    target.port.is_some()
        || target.socket.as_ref().is_some_and(|s| !s.is_empty())
        || target
            .docker_container
            .as_ref()
            .is_some_and(|s| !s.is_empty())
}

fn endpoint_from_target(target: &RedisTargetConfig) -> RedisEndpoint {
    RedisEndpoint {
        label: target
            .label
            .clone()
            .unwrap_or_else(|| "configured".into()),
        host: Some("127.0.0.1".into()),
        port: target.port,
        socket: target.socket.clone(),
        docker_container: target.docker_container.clone(),
        bind: target.port.map(|port| format!("127.0.0.1:{port}")),
        tls: false,
    }
}

fn choose_endpoint(endpoints: &[RedisEndpoint], saved: &RedisTargetConfig) -> RedisEndpoint {
    if let Some(label) = &saved.label {
        if let Some(found) = endpoints.iter().find(|e| &e.label == label) {
            return found.clone();
        }
    }
    if target_configured(saved) {
        return endpoint_from_target(saved);
    }
    endpoints
        .first()
        .cloned()
        .unwrap_or_else(|| endpoint_from_target(saved))
}

fn classify_redis_error(msg: &str) -> String {
    let lower = msg.to_lowercase();
    if lower.contains("noauth")
        || lower.contains("wrongpass")
        || lower.contains("authentication")
    {
        "auth_required".into()
    } else if lower.contains("connection refused") || lower.contains("could not connect") {
        "connection_refused".into()
    } else if lower.contains("not found") || lower.contains("no such file") {
        "not_installed".into()
    } else if lower.contains("timed out") || lower.contains("124") {
        "timed_out".into()
    } else {
        "timed_out".into()
    }
}

fn push_redis_phase(
    event_hub: &Arc<EventHub>,
    server_id: &str,
    phase: &str,
    message: Option<String>,
    instances: &[RedisEndpoint],
) {
    let snap = serde_json::json!({
        "collectedAtMs": 0,
        "info": {},
        "keyspace": [],
        "slowlog": [],
        "instances": instances,
        "derived": {
            "opsPerSec": 0,
            "hitRatePercent": 0,
            "memoryUsedBytes": 0,
            "memoryPeakBytes": 0,
            "maxmemoryBytes": 0,
            "connectedClients": 0,
            "evictedKeys": 0,
            "expiredKeys": 0,
            "role": "—",
            "totalKeys": 0
        },
        "collectorState": { "phase": phase, "message": message }
    });
    event_hub.push_redis(server_id, snap);
}

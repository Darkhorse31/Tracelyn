use crate::collectors::redis::RedisCollector;
use crate::collectors::system::SystemCollectorState;
use crate::config::ServerRecord;
use crate::connection::ssh_runner::SessionRunner;
use crate::connection::web_pipeline::{redis_detected, WebPipeline};
use crate::events::EventHub;
use crate::metrics_ingest::{
    evaluate_live_alerts, persist_samples, samples_from_overview, samples_from_redis,
    samples_from_web,
};
use crate::ssh::{connect_persistent, run_command};
use crate::ssh::SshSession;
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

                if let Err(err) = collector_loop(&app, &session, &server_id, &event_hub).await {
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
                let msg = err.to_string();
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
    let mut web = WebPipeline::setup(session).await;
    let redis_available = redis_detected(session).await;
    let mut redis_collector = if redis_available {
        Some(RedisCollector::new(SessionRunner::new(session), server_id))
    } else {
        None
    };

    let mut fast = tokio::time::interval(Duration::from_secs(2));
    let mut slow = tokio::time::interval(Duration::from_secs(5));
    let mut services_iv = tokio::time::interval(Duration::from_secs(15));
    let mut web_iv = tokio::time::interval(Duration::from_secs(1));
    let mut redis_iv = tokio::time::interval(Duration::from_secs(10));
    fast.tick().await;
    slow.tick().await;
    services_iv.tick().await;
    web_iv.tick().await;
    redis_iv.tick().await;

    loop {
        web.drain_log_lines();

        tokio::select! {
            _ = fast.tick() => {
                let err = state.poll_fast(session).await.err();
                let overview = state.overview(server_id);
                event_hub.push_overview(overview.clone());
                event_hub.push_server_detail(state.snapshot(server_id, err));

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
                event_hub.push_server_detail(state.snapshot(server_id, None));
            }
            _ = services_iv.tick() => {
                let _ = state.poll_services(session).await;
                event_hub.push_server_detail(state.snapshot(server_id, None));
            }
            _ = web_iv.tick() => {
                let (web_event, apm_event) = web.build_events(server_id);
                let req_per_sec = web_event.metrics.req_per_sec;
                event_hub.push_web_traffic(web_event);
                event_hub.push_apm(apm_event);
                let state_app = app.state::<AppState>();
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
            _ = redis_iv.tick(), if redis_collector.is_some() => {
                if let Some(collector) = redis_collector.as_mut() {
                    if let Ok(snap) = collector.poll().await {
                        if let Ok(json) = serde_json::to_value(&snap) {
                            event_hub.push_redis(server_id, json);
                        }
                        let state_app = app.state::<AppState>();
                        persist_samples(&state_app, samples_from_redis(server_id, &snap));
                        let mut alert_metrics = HashMap::new();
                        if let Some(ratio) = snap.derived.memory_used_ratio {
                            alert_metrics.insert("redis.memory_used_ratio".into(), ratio);
                        }
                        evaluate_live_alerts(app, server_id, true, alert_metrics);
                    }
                }
            }
        }
    }
}

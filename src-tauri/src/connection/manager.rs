use crate::collectors::system::SystemCollectorState;
use crate::config::ServerRecord;
use crate::events::EventHub;
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

                if let Err(err) = collector_loop(&session, &server_id, &event_hub).await {
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
    session: &SshSession,
    server_id: &str,
    event_hub: &Arc<EventHub>,
) -> Result<(), String> {
    let mut state = SystemCollectorState::new();
    let mut fast = tokio::time::interval(Duration::from_secs(2));
    let mut slow = tokio::time::interval(Duration::from_secs(5));
    let mut services_iv = tokio::time::interval(Duration::from_secs(15));
    fast.tick().await;
    slow.tick().await;
    services_iv.tick().await;

    loop {
        tokio::select! {
            _ = fast.tick() => {
                let err = state.poll_fast(session).await.err();
                event_hub.push_overview(state.overview(server_id));
                event_hub.push_server_detail(state.snapshot(server_id, err));
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
        }
    }
}

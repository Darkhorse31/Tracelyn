use crate::apps::TracelynApp;
use crate::events::payloads::{ApmMetricsEvent, WebTrafficEvent};
use crate::logs::LogLine;
use crate::metrics::{OverviewServerMetrics, ServerMetricsSnapshot};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const FLUSH_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStreamSnapshot {
    pub lines: Vec<LogLine>,
    /// False for the initial window from `get_log_snapshot`. True for live batches.
    pub incremental: bool,
    pub paused: bool,
    pub source_status: HashMap<String, SourceStatus>,
    pub facets: LogFacets,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub ok: bool,
    pub reason: Option<String>,
    pub fix_hint: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFacets {
    pub level: HashMap<String, u64>,
    pub app_id: HashMap<String, u64>,
}

pub struct EventHub {
    overview_subscribers: Mutex<u32>,
    server_detail_subscribers: Mutex<HashSet<String>>,
    web_traffic_subscribers: Mutex<HashSet<String>>,
    apm_subscribers: Mutex<HashSet<String>>,
    redis_subscribers: Mutex<HashSet<String>>,
    apps_subscribers: Mutex<HashSet<String>>,
    log_stream_subscribers: Mutex<HashSet<String>>,
    pending_overview: Mutex<HashMap<String, OverviewServerMetrics>>,
    pending_server: Mutex<HashMap<String, ServerMetricsSnapshot>>,
    pending_web: Mutex<HashMap<String, WebTrafficEvent>>,
    pending_apm: Mutex<HashMap<String, ApmMetricsEvent>>,
    pending_redis: Mutex<HashMap<String, serde_json::Value>>,
    pending_apps: Mutex<HashMap<String, Vec<TracelynApp>>>,
    pending_logs: Mutex<HashMap<String, LogStreamSnapshot>>,
    last_web: Mutex<HashMap<String, WebTrafficEvent>>,
    last_apm: Mutex<HashMap<String, ApmMetricsEvent>>,
    last_redis: Mutex<HashMap<String, serde_json::Value>>,
    last_apps: Mutex<HashMap<String, Vec<TracelynApp>>>,
    last_logs: Mutex<HashMap<String, LogStreamSnapshot>>,
    app: Mutex<Option<AppHandle>>,
}

impl EventHub {
    pub fn new() -> Self {
        Self {
            overview_subscribers: Mutex::new(0),
            server_detail_subscribers: Mutex::new(HashSet::new()),
            web_traffic_subscribers: Mutex::new(HashSet::new()),
            apm_subscribers: Mutex::new(HashSet::new()),
            redis_subscribers: Mutex::new(HashSet::new()),
            apps_subscribers: Mutex::new(HashSet::new()),
            log_stream_subscribers: Mutex::new(HashSet::new()),
            pending_overview: Mutex::new(HashMap::new()),
            pending_server: Mutex::new(HashMap::new()),
            pending_web: Mutex::new(HashMap::new()),
            pending_apm: Mutex::new(HashMap::new()),
            pending_redis: Mutex::new(HashMap::new()),
            pending_apps: Mutex::new(HashMap::new()),
            pending_logs: Mutex::new(HashMap::new()),
            last_web: Mutex::new(HashMap::new()),
            last_apm: Mutex::new(HashMap::new()),
            last_redis: Mutex::new(HashMap::new()),
            last_apps: Mutex::new(HashMap::new()),
            last_logs: Mutex::new(HashMap::new()),
            app: Mutex::new(None),
        }
    }

    pub fn set_app(&self, app: AppHandle) {
        *self.app.lock() = Some(app);
    }

    pub fn subscribe_overview(&self) {
        *self.overview_subscribers.lock() += 1;
    }

    pub fn unsubscribe_overview(&self) {
        let mut n = self.overview_subscribers.lock();
        *n = n.saturating_sub(1);
    }

    pub fn subscribe_server_detail(&self, server_id: &str) {
        self.server_detail_subscribers.lock().insert(server_id.to_string());
    }

    pub fn unsubscribe_server_detail(&self, server_id: &str) {
        self.server_detail_subscribers.lock().remove(server_id);
    }

    pub fn subscribe_web_traffic(&self, server_id: &str) {
        self.web_traffic_subscribers.lock().insert(server_id.to_string());
        self.replay_web(server_id);
    }

    pub fn unsubscribe_web_traffic(&self, server_id: &str) {
        self.web_traffic_subscribers.lock().remove(server_id);
    }

    pub fn subscribe_apm(&self, server_id: &str) {
        self.apm_subscribers.lock().insert(server_id.to_string());
        self.replay_apm(server_id);
    }

    pub fn unsubscribe_apm(&self, server_id: &str) {
        self.apm_subscribers.lock().remove(server_id);
    }

    pub fn subscribe_redis(&self, server_id: &str) {
        self.redis_subscribers.lock().insert(server_id.to_string());
        self.replay_redis(server_id);
    }

    pub fn unsubscribe_redis(&self, server_id: &str) {
        self.redis_subscribers.lock().remove(server_id);
    }

    pub fn subscribe_apps(&self, server_id: &str) {
        self.apps_subscribers.lock().insert(server_id.to_string());
        self.replay_apps(server_id);
    }

    pub fn unsubscribe_apps(&self, server_id: &str) {
        self.apps_subscribers.lock().remove(server_id);
    }

    pub fn subscribe_log_stream(&self, server_id: &str) {
        self.log_stream_subscribers.lock().insert(server_id.to_string());
        self.replay_logs(server_id);
    }

    pub fn unsubscribe_log_stream(&self, server_id: &str) {
        self.log_stream_subscribers.lock().remove(server_id);
    }

    pub fn replay_web(&self, server_id: &str) {
        if let Some(event) = self.last_web.lock().get(server_id).cloned() {
            self.pending_web.lock().insert(server_id.to_string(), event);
        }
    }

    pub fn replay_apm(&self, server_id: &str) {
        if let Some(event) = self.last_apm.lock().get(server_id).cloned() {
            self.pending_apm.lock().insert(server_id.to_string(), event);
        }
    }

    pub fn replay_redis(&self, server_id: &str) {
        if let Some(snapshot) = self.last_redis.lock().get(server_id).cloned() {
            self.pending_redis.lock().insert(server_id.to_string(), snapshot);
        }
    }

    pub fn replay_apps(&self, server_id: &str) {
        if let Some(apps) = self.last_apps.lock().get(server_id).cloned() {
            self.pending_apps.lock().insert(server_id.to_string(), apps);
        }
    }

    pub fn replay_logs(&self, server_id: &str) {
        if let Some(snap) = self.last_logs.lock().get(server_id).cloned() {
            self.pending_logs.lock().insert(server_id.to_string(), snap);
        }
    }

    pub fn last_log_snapshot(&self, server_id: &str) -> Option<LogStreamSnapshot> {
        self.last_logs.lock().get(server_id).cloned()
    }

    pub fn last_redis_snapshot(&self, server_id: &str) -> Option<serde_json::Value> {
        self.last_redis.lock().get(server_id).cloned()
    }

    pub fn last_web_snapshot(&self, server_id: &str) -> Option<crate::events::payloads::WebTrafficEvent> {
        self.last_web.lock().get(server_id).cloned()
    }

    pub fn last_apm_snapshot(&self, server_id: &str) -> Option<crate::events::payloads::ApmMetricsEvent> {
        self.last_apm.lock().get(server_id).cloned()
    }

    pub fn push_overview(&self, metrics: OverviewServerMetrics) {
        if *self.overview_subscribers.lock() == 0 {
            return;
        }
        self.pending_overview
            .lock()
            .insert(metrics.server_id.clone(), metrics);
    }

    pub fn push_server_detail(&self, snapshot: ServerMetricsSnapshot) {
        if !self
            .server_detail_subscribers
            .lock()
            .contains(&snapshot.server_id)
        {
            return;
        }
        self.pending_server
            .lock()
            .insert(snapshot.server_id.clone(), snapshot);
    }

    pub fn push_web_traffic(&self, event: WebTrafficEvent) {
        self.last_web
            .lock()
            .insert(event.server_id.clone(), event.clone());
        if self
            .web_traffic_subscribers
            .lock()
            .contains(&event.server_id)
        {
            self.pending_web.lock().insert(event.server_id.clone(), event);
        }
    }

    pub fn push_apm(&self, event: ApmMetricsEvent) {
        self.last_apm
            .lock()
            .insert(event.server_id.clone(), event.clone());
        if self.apm_subscribers.lock().contains(&event.server_id) {
            self.pending_apm.lock().insert(event.server_id.clone(), event);
        }
    }

    pub fn push_redis(&self, server_id: &str, snapshot: serde_json::Value) {
        self.last_redis
            .lock()
            .insert(server_id.to_string(), snapshot.clone());
        if self.redis_subscribers.lock().contains(server_id) {
            self.pending_redis
                .lock()
                .insert(server_id.to_string(), snapshot);
        }
    }

    pub fn push_apps(&self, server_id: &str, apps: Vec<TracelynApp>) {
        self.last_apps.lock().insert(server_id.to_string(), apps.clone());
        if self.apps_subscribers.lock().contains(server_id) {
            self.pending_apps.lock().insert(server_id.to_string(), apps);
        }
    }

    pub fn push_log_stream(&self, server_id: &str, snapshot: LogStreamSnapshot) {
        self.last_logs
            .lock()
            .insert(server_id.to_string(), snapshot.clone());
        if self.log_stream_subscribers.lock().contains(server_id) {
            self.pending_logs.lock().insert(server_id.to_string(), snapshot);
        }
    }

    pub fn spawn_flusher(self: Arc<Self>) {
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(FLUSH_INTERVAL);
            loop {
                interval.tick().await;
                self.flush();
            }
        });
    }

    fn flush(&self) {
        let app = self.app.lock().clone();
        let Some(app) = app else {
            return;
        };

        if *self.overview_subscribers.lock() > 0 {
            let batch: Vec<OverviewServerMetrics> = self
                .pending_overview
                .lock()
                .drain()
                .map(|(_, v)| v)
                .collect();
            if !batch.is_empty() {
                let _ = app.emit("overview-metrics", &batch);
            }
        }

        let subs = self.server_detail_subscribers.lock().clone();
        if !subs.is_empty() {
            let mut pending = self.pending_server.lock();
            for id in subs {
                if let Some(snapshot) = pending.remove(&id) {
                    let _ = app.emit(&format!("server-metrics:{}", id), &snapshot);
                }
            }
        }

        let web_subs = self.web_traffic_subscribers.lock().clone();
        if !web_subs.is_empty() {
            let mut pending = self.pending_web.lock();
            for id in web_subs {
                if let Some(event) = pending.remove(&id) {
                    let _ = app.emit(&format!("web-traffic:{}", id), &event);
                }
            }
        }

        let apm_subs = self.apm_subscribers.lock().clone();
        if !apm_subs.is_empty() {
            let mut pending = self.pending_apm.lock();
            for id in apm_subs {
                if let Some(event) = pending.remove(&id) {
                    let _ = app.emit(&format!("apm-metrics:{}", id), &event);
                }
            }
        }

        let redis_subs = self.redis_subscribers.lock().clone();
        if !redis_subs.is_empty() {
            let mut pending = self.pending_redis.lock();
            for id in redis_subs {
                if let Some(snapshot) = pending.remove(&id) {
                    let _ = app.emit(&format!("redis-metrics:{}", id), &snapshot);
                }
            }
        }

        let apps_subs = self.apps_subscribers.lock().clone();
        if !apps_subs.is_empty() {
            let mut pending = self.pending_apps.lock();
            for id in apps_subs {
                if let Some(apps) = pending.remove(&id) {
                    let _ = app.emit(&format!("apps-updated:{}", id), &apps);
                }
            }
        }

        let log_subs = self.log_stream_subscribers.lock().clone();
        if !log_subs.is_empty() {
            let mut pending = self.pending_logs.lock();
            for id in log_subs {
                if let Some(snap) = pending.remove(&id) {
                    let _ = app.emit(&format!("log-stream:{}", id), &snap);
                }
            }
        }
    }
}

use crate::events::payloads::{ApmMetricsEvent, WebTrafficEvent};
use crate::metrics::{OverviewServerMetrics, ServerMetricsSnapshot};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const FLUSH_INTERVAL: Duration = Duration::from_millis(250);

pub struct EventHub {
    overview_subscribers: Mutex<u32>,
    server_detail_subscribers: Mutex<HashSet<String>>,
    web_traffic_subscribers: Mutex<HashSet<String>>,
    apm_subscribers: Mutex<HashSet<String>>,
    redis_subscribers: Mutex<HashSet<String>>,
    pending_overview: Mutex<HashMap<String, OverviewServerMetrics>>,
    pending_server: Mutex<HashMap<String, ServerMetricsSnapshot>>,
    pending_web: Mutex<HashMap<String, WebTrafficEvent>>,
    pending_apm: Mutex<HashMap<String, ApmMetricsEvent>>,
    pending_redis: Mutex<HashMap<String, serde_json::Value>>,
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
            pending_overview: Mutex::new(HashMap::new()),
            pending_server: Mutex::new(HashMap::new()),
            pending_web: Mutex::new(HashMap::new()),
            pending_apm: Mutex::new(HashMap::new()),
            pending_redis: Mutex::new(HashMap::new()),
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
    }

    pub fn unsubscribe_web_traffic(&self, server_id: &str) {
        self.web_traffic_subscribers.lock().remove(server_id);
    }

    pub fn subscribe_apm(&self, server_id: &str) {
        self.apm_subscribers.lock().insert(server_id.to_string());
    }

    pub fn unsubscribe_apm(&self, server_id: &str) {
        self.apm_subscribers.lock().remove(server_id);
    }

    pub fn subscribe_redis(&self, server_id: &str) {
        self.redis_subscribers.lock().insert(server_id.to_string());
    }

    pub fn unsubscribe_redis(&self, server_id: &str) {
        self.redis_subscribers.lock().remove(server_id);
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
        if !self
            .web_traffic_subscribers
            .lock()
            .contains(&event.server_id)
        {
            return;
        }
        self.pending_web.lock().insert(event.server_id.clone(), event);
    }

    pub fn push_apm(&self, event: ApmMetricsEvent) {
        if !self.apm_subscribers.lock().contains(&event.server_id) {
            return;
        }
        self.pending_apm.lock().insert(event.server_id.clone(), event);
    }

    pub fn push_redis(&self, server_id: &str, snapshot: serde_json::Value) {
        if !self.redis_subscribers.lock().contains(server_id) {
            return;
        }
        self.pending_redis.lock().insert(server_id.to_string(), snapshot);
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
    }
}

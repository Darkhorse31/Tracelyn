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
    pending_overview: Mutex<HashMap<String, OverviewServerMetrics>>,
    pending_server: Mutex<HashMap<String, ServerMetricsSnapshot>>,
    app: Mutex<Option<AppHandle>>,
}

impl EventHub {
    pub fn new() -> Self {
        Self {
            overview_subscribers: Mutex::new(0),
            server_detail_subscribers: Mutex::new(HashSet::new()),
            pending_overview: Mutex::new(HashMap::new()),
            pending_server: Mutex::new(HashMap::new()),
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
            let batch: Vec<OverviewServerMetrics> = self.pending_overview.lock().drain().map(|(_, v)| v).collect();
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
    }
}

use super::parse::AccessLogEntry;
use super::quantile::StreamingQuantiles;
use super::route::RouteNormalizer;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

const TOP_N: usize = 10;
const LOG_BUFFER_CAP: usize = 2000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficSnapshot {
    pub req_per_sec: f64,
    pub status_counts: HashMap<u16, u64>,
    pub top_urls: Vec<(String, u64)>,
    pub top_ips: Vec<(String, u64)>,
    pub bandwidth_bytes_per_sec: f64,
    pub window_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointMetrics {
    pub route: String,
    pub throughput: f64,
    pub avg_ms: Option<f64>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub error_rate: f64,
    pub status_counts: HashMap<u16, u64>,
    pub request_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointSnapshot {
    pub endpoints: Vec<EndpointMetrics>,
    pub heatmap: Vec<HeatmapCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatmapCell {
    pub route: String,
    pub bucket_secs: u64,
    pub p50_ms: f64,
    pub p95_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLineBuffer {
    pub lines: VecDeque<BufferedLogLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BufferedLogLine {
    pub id: u64,
    pub timestamp_secs: u64,
    pub status: u16,
    pub path: String,
    pub route: String,
    pub remote_addr: String,
    pub method: String,
    pub request_time_ms: Option<f64>,
    pub raw_line: String,
}

pub struct TrafficAggregator {
    window_secs: u64,
    bucket_start: u64,
    requests_in_window: u64,
    bytes_in_window: u64,
    status_counts: HashMap<u16, u64>,
    url_counts: HashMap<String, u64>,
    ip_counts: HashMap<String, u64>,
    route_normalizer: RouteNormalizer,
    endpoint_stats: HashMap<String, EndpointState>,
    heatmap: HashMap<(String, u64), StreamingQuantiles>,
    log_buffer: LogLineBuffer,
    next_line_id: u64,
}

struct EndpointState {
    requests: u64,
    errors: u64,
    status_counts: HashMap<u16, u64>,
    latency: StreamingQuantiles,
}

impl TrafficAggregator {
    pub fn new(window_secs: u64, normalizer: RouteNormalizer) -> Self {
        let now = now_secs();
        Self {
            window_secs,
            bucket_start: now,
            requests_in_window: 0,
            bytes_in_window: 0,
            status_counts: HashMap::new(),
            url_counts: HashMap::new(),
            ip_counts: HashMap::new(),
            route_normalizer: normalizer,
            endpoint_stats: HashMap::new(),
            heatmap: HashMap::new(),
            log_buffer: LogLineBuffer {
                lines: VecDeque::new(),
            },
            next_line_id: 1,
        }
    }

    pub fn ingest(&mut self, entry: &AccessLogEntry, timestamp_secs: u64) {
        self.roll_window(timestamp_secs);
        self.requests_in_window += 1;
        self.bytes_in_window += entry.bytes_sent;
        *self.status_counts.entry(entry.status).or_insert(0) += 1;
        *self.url_counts.entry(entry.path.clone()).or_insert(0) += 1;
        *self.ip_counts.entry(entry.remote_addr.clone()).or_insert(0) += 1;

        let route = self.route_normalizer.normalize(&entry.path);
        let ep = self
            .endpoint_stats
            .entry(route.clone())
            .or_insert_with(|| EndpointState {
                requests: 0,
                errors: 0,
                status_counts: HashMap::new(),
                latency: StreamingQuantiles::new(),
            });
        ep.requests += 1;
        if entry.status >= 500 {
            ep.errors += 1;
        }
        *ep.status_counts.entry(entry.status).or_insert(0) += 1;
        if let Some(secs) = entry.request_time_secs {
            ep.latency.observe(secs);
            let bucket = timestamp_secs / 60;
            self.heatmap
                .entry((route.clone(), bucket))
                .or_insert_with(StreamingQuantiles::new)
                .observe(secs);
        }

        let line = BufferedLogLine {
            id: self.next_line_id,
            timestamp_secs,
            status: entry.status,
            path: entry.path.clone(),
            route,
            remote_addr: entry.remote_addr.clone(),
            method: entry.method.clone(),
            request_time_ms: entry.request_time_secs.map(|s| s * 1000.0),
            raw_line: entry.raw_line.clone(),
        };
        self.next_line_id += 1;
        self.log_buffer.lines.push_back(line);
        while self.log_buffer.lines.len() > LOG_BUFFER_CAP {
            self.log_buffer.lines.pop_front();
        }
    }

    fn roll_window(&mut self, now: u64) {
        if now >= self.bucket_start + self.window_secs {
            self.bucket_start = now;
            self.requests_in_window = 0;
            self.bytes_in_window = 0;
            self.status_counts.clear();
            self.url_counts.clear();
            self.ip_counts.clear();
        }
    }

    pub fn traffic_snapshot(&self) -> TrafficSnapshot {
        let elapsed = self.window_secs.max(1) as f64;
        TrafficSnapshot {
            req_per_sec: self.requests_in_window as f64 / elapsed,
            status_counts: self.status_counts.clone(),
            top_urls: top_k(&self.url_counts, TOP_N),
            top_ips: top_k(&self.ip_counts, TOP_N),
            bandwidth_bytes_per_sec: self.bytes_in_window as f64 / elapsed,
            window_secs: self.window_secs,
        }
    }

    pub fn endpoint_snapshot(&self) -> EndpointSnapshot {
        let mut endpoints: Vec<EndpointMetrics> = self
            .endpoint_stats
            .iter()
            .map(|(route, st)| {
                let err_rate = if st.requests == 0 {
                    0.0
                } else {
                    st.errors as f64 / st.requests as f64
                };
                EndpointMetrics {
                    route: route.clone(),
                    throughput: st.requests as f64 / self.window_secs.max(1) as f64,
                    avg_ms: st.latency.avg_ms(),
                    p50_ms: st.latency.p50_ms(),
                    p95_ms: st.latency.p95_ms(),
                    p99_ms: st.latency.p99_ms(),
                    error_rate: err_rate,
                    status_counts: st.status_counts.clone(),
                    request_count: st.requests,
                }
            })
            .collect();
        endpoints.sort_by(|a, b| b.request_count.cmp(&a.request_count));

        let heatmap: Vec<HeatmapCell> = self
            .heatmap
            .iter()
            .filter_map(|((route, bucket), q)| {
                let p50 = q.p50_ms()?;
                let p95 = q.p95_ms()?;
                Some(HeatmapCell {
                    route: route.clone(),
                    bucket_secs: bucket * 60,
                    p50_ms: p50,
                    p95_ms: p95,
                })
            })
            .collect();

        EndpointSnapshot {
            endpoints,
            heatmap,
        }
    }

    pub fn log_buffer(&self) -> &LogLineBuffer {
        &self.log_buffer
    }
}

fn top_k(map: &HashMap<String, u64>, n: usize) -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = map
        .iter()
        .map(|(k, c)| (k.clone(), *c))
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.truncate(n);
    v
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

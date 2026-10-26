use crate::events::payloads::{
    ApmEndpointRow, ApmHeatmapCell, ApmMetricsEvent, LiveLogLinePayload, MetricPointPayload,
    TimingNoticePayload, TopIpEntry, TopUrlEntry, WebTrafficEvent, WebTrafficMetricsPayload,
};
use crate::ssh::{run_command, start_tail_follow, SshSession};
use crate::webserver::{
    detect_and_parse_config, parse_access_line, suggest_apache_timing_line,
    suggest_nginx_timing_line, LogFormatKind, ParsedWebConfig, RouteNormalizer, RouteRule,
    TrafficAggregator, HEATMAP_MINUTES,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::Receiver;

const SERIES_LEN: usize = 60;

pub struct WebPipeline {
    aggregator: TrafficAggregator,
    log_format: LogFormatKind,
    timing_notice: Option<TimingNoticePayload>,
    permission_denied: bool,
    web_detected: bool,
    req_series: VecDeque<MetricPointPayload>,
    log_rxs: Vec<LogTail>,
    access_log_paths: Vec<String>,
    unreadable_logs: Vec<String>,
    path_stats: HashMap<String, PathStat>,
    recent_access: Vec<(String, String)>,
}

struct LogTail {
    path: String,
    rx: Receiver<Result<String, String>>,
}

struct PathStat {
    window_reqs: u64,
    window_err: u64,
    latencies_ms: Vec<f64>,
    req_per_sec: f64,
    error_rate: f64,
    p95_ms: Option<f64>,
}

impl WebPipeline {
    pub async fn setup(session: &SshSession, route_rules: Vec<RouteRule>) -> Self {
        let runner = crate::connection::ssh_runner::SessionRunner::new(session);
        let detected = detect_and_parse_config(&runner).await.ok().flatten();
        let mut permission_denied = false;
        let mut web_detected = false;
        let mut log_format = LogFormatKind::Combined;
        let mut timing_notice = None;
        let mut log_rxs = Vec::new();
        let mut access_log_paths = Vec::new();
        let mut unreadable_logs = Vec::new();

        if let Some(detected) = detected {
            web_detected = true;
            let (paths, notice) = match &detected.config {
                Some(ParsedWebConfig::Nginx(info)) => {
                    if !info.request_timing.present {
                        timing_notice = Some(TimingNoticePayload {
                            message:
                                "Response time is not in the Nginx access log. Add a log_format with $request_time and $upstream_response_time. Then run nginx -t and reload Nginx."
                                    .into(),
                            suggested_line: info
                                .request_timing
                                .suggested_line
                                .clone()
                                .unwrap_or_else(|| {
                                    suggest_nginx_timing_line(
                                        info.access_log_format_name.as_deref().unwrap_or("timed"),
                                    )
                                }),
                        });
                    }
                    log_format = if info.request_timing.present {
                        LogFormatKind::Custom
                    } else {
                        LogFormatKind::Combined
                    };
                    (dedupe_paths(&info.access_log_paths), timing_notice.clone())
                }
                Some(ParsedWebConfig::Apache(info)) => {
                    if !info.request_timing.present {
                        timing_notice = Some(TimingNoticePayload {
                            message:
                                "Response time is not in the Apache access log. Add a LogFormat named combined_timing with %D. Then run apache2ctl configtest and reload Apache."
                                    .into(),
                            suggested_line: info
                                .request_timing
                                .suggested_line
                                .clone()
                                .unwrap_or_else(suggest_apache_timing_line),
                        });
                    }
                    log_format = if info.request_timing.present {
                        LogFormatKind::ApacheTiming
                    } else {
                        LogFormatKind::Combined
                    };
                    (dedupe_paths(&info.access_log_paths), timing_notice.clone())
                }
                None => (vec![], None),
            };
            timing_notice = notice;
            access_log_paths = paths.clone();

            let max_tails = 24usize;
            for path in paths.into_iter().take(max_tails) {
                let readable = run_command(
                    session,
                    &format!("test -r '{}' && echo ok", path.replace('\'', "'\\''")),
                )
                .await
                .map(|s| s.trim() == "ok")
                .unwrap_or(false);
                if readable {
                    if let Ok(rx) = start_tail_follow(session, &path).await {
                        log_rxs.push(LogTail { path, rx });
                    }
                } else {
                    permission_denied = true;
                    unreadable_logs.push(path);
                }
            }
        }

        let normalizer = RouteNormalizer::new(route_rules);
        Self {
            aggregator: TrafficAggregator::new(60, normalizer),
            log_format,
            timing_notice,
            permission_denied,
            web_detected,
            req_series: VecDeque::new(),
            log_rxs,
            access_log_paths,
            unreadable_logs,
            path_stats: HashMap::new(),
            recent_access: Vec::new(),
        }
    }

    pub fn unreadable_logs(&self) -> &[String] {
        &self.unreadable_logs
    }

    pub fn take_recent_access(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.recent_access)
    }

    /// Per access-log path: req/s, error rate (0-1), p95 ms. Rolled on each call.
    pub fn path_metrics(&mut self) -> HashMap<String, (f64, f64, Option<f64>)> {
        let mut out = HashMap::new();
        for (path, stat) in &mut self.path_stats {
            stat.req_per_sec = stat.window_reqs as f64;
            stat.error_rate = if stat.window_reqs == 0 {
                0.0
            } else {
                stat.window_err as f64 / stat.window_reqs as f64
            };
            stat.p95_ms = percentile95(&stat.latencies_ms);
            stat.window_reqs = 0;
            stat.window_err = 0;
            stat.latencies_ms.clear();
            out.insert(path.clone(), (stat.req_per_sec, stat.error_rate, stat.p95_ms));
        }
        out
    }

    pub fn access_log_paths(&self) -> &[String] {
        &self.access_log_paths
    }

    pub fn logs_readable(&self) -> bool {
        !self.log_rxs.is_empty()
    }

    pub fn web_detected(&self) -> bool {
        self.web_detected
    }

    /// Applies to lines ingested from now on; existing endpoint stats keep their routes.
    pub fn set_route_rules(&mut self, rules: Vec<RouteRule>) {
        let normalizer = self.aggregator.route_normalizer_mut();
        if normalizer.rules() != rules.as_slice() {
            normalizer.set_rules(rules);
        }
    }

    fn ingest_from(&mut self, line: &str, path: Option<&str>) {
        if let Ok(entry) = parse_access_line(line, self.log_format) {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if let Some(path) = path {
                let stat = self.path_stats.entry(path.to_string()).or_insert_with(|| PathStat {
                    window_reqs: 0,
                    window_err: 0,
                    latencies_ms: Vec::new(),
                    req_per_sec: 0.0,
                    error_rate: 0.0,
                    p95_ms: None,
                });
                stat.window_reqs += 1;
                if entry.status >= 500 {
                    stat.window_err += 1;
                }
                if let Some(secs) = entry.request_time_secs {
                    if stat.latencies_ms.len() < 128 {
                        stat.latencies_ms.push(secs * 1000.0);
                    }
                }
            }
            self.aggregator.ingest(&entry, ts);
        }
    }

    pub fn drain_log_lines(&mut self) {
        self.recent_access.clear();
        let mut batch = Vec::new();
        for tail in &mut self.log_rxs {
            while let Ok(line) = tail.rx.try_recv() {
                batch.push((tail.path.clone(), line));
            }
        }
        for (path, line) in batch {
            match line {
                Ok(l) => {
                    if self.recent_access.len() < 2000 {
                        self.recent_access.push((path.clone(), l.clone()));
                    }
                    self.ingest_from(&l, Some(&path));
                }
                Err(msg) => {
                    if msg.to_lowercase().contains("permission denied") {
                        self.permission_denied = true;
                        if !self.unreadable_logs.contains(&path) {
                            self.unreadable_logs.push(path);
                        }
                    }
                }
            }
        }
    }

    pub fn build_events(&mut self, server_id: &str) -> (WebTrafficEvent, ApmMetricsEvent) {
        let traffic = self.aggregator.traffic_snapshot();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        self.req_series.push_back(MetricPointPayload {
            t: now_ms,
            v: traffic.req_per_sec,
        });
        while self.req_series.len() > SERIES_LEN {
            self.req_series.pop_front();
        }

        let status_breakdown = status_family_counts(&traffic.status_counts);
        let bandwidth_mbps = traffic.bandwidth_bytes_per_sec * 8.0 / 1_000_000.0;

        let metrics = WebTrafficMetricsPayload {
            req_per_sec: traffic.req_per_sec,
            bandwidth_mbps,
            status_breakdown,
            top_urls: traffic
                .top_urls
                .iter()
                .map(|(p, c)| TopUrlEntry {
                    path: p.clone(),
                    count: *c,
                })
                .collect(),
            top_ips: traffic
                .top_ips
                .iter()
                .map(|(ip, c)| TopIpEntry {
                    ip: ip.clone(),
                    count: *c,
                })
                .collect(),
            req_per_sec_series: self.req_series.iter().cloned().collect(),
            timing_notice: self.timing_notice.clone(),
        };

        let logs: Vec<LiveLogLinePayload> = self
            .aggregator
            .log_buffer()
            .lines
            .iter()
            .map(|l| LiveLogLinePayload {
                id: l.id.to_string(),
                timestamp: l.timestamp_secs * 1000,
                status: l.status,
                method: l.method.clone(),
                path: l.path.clone(),
                remote_addr: l.remote_addr.clone(),
                latency_ms: l.request_time_ms,
                raw: l.raw_line.clone(),
            })
            .collect();

        let web = WebTrafficEvent {
            server_id: server_id.to_string(),
            metrics,
            logs,
            permission_denied: self.permission_denied,
            web_detected: self.web_detected,
        };

        let ep = self.aggregator.endpoint_snapshot();
        let endpoints: Vec<ApmEndpointRow> = ep
            .endpoints
            .iter()
            .map(|e| {
                let status_breakdown: HashMap<String, u64> = e
                    .status_counts
                    .iter()
                    .map(|(k, v)| (k.to_string(), *v))
                    .collect();
                ApmEndpointRow {
                    route: e.route.clone(),
                    throughput: e.throughput,
                    avg_ms: e.avg_ms.unwrap_or(0.0),
                    p50_ms: e.p50_ms.unwrap_or(0.0),
                    p95_ms: e.p95_ms.unwrap_or(0.0),
                    p99_ms: e.p99_ms.unwrap_or(0.0),
                    error_rate: e.error_rate,
                    status_breakdown,
                }
            })
            .collect();

        // Index 0 is the oldest minute in the window, HEATMAP_MINUTES - 1 the current one.
        let now_min = now_ms / 60_000;
        let heatmap: Vec<ApmHeatmapCell> = ep
            .heatmap
            .iter()
            .filter_map(|c| {
                let age = now_min.checked_sub(c.bucket_secs / 60)?;
                (age < HEATMAP_MINUTES).then(|| ApmHeatmapCell {
                    route: c.route.clone(),
                    bucket_index: (HEATMAP_MINUTES - 1 - age) as u32,
                    p50_ms: c.p50_ms,
                    p95_ms: c.p95_ms,
                })
            })
            .collect();

        let apm = ApmMetricsEvent {
            server_id: server_id.to_string(),
            endpoints,
            heatmap,
            timing_notice: self.timing_notice.clone(),
        };

        (web, apm)
    }

    pub fn http_5xx_rate_percent(&self) -> f64 {
        let traffic = self.aggregator.traffic_snapshot();
        let total: u64 = traffic.status_counts.values().sum();
        if total == 0 {
            return 0.0;
        }
        let errors: u64 = traffic
            .status_counts
            .iter()
            .filter(|(code, _)| **code >= 500)
            .map(|(_, c)| *c)
            .sum();
        errors as f64 / total as f64 * 100.0
    }

    pub fn max_endpoint_p95_ms(&self) -> f64 {
        self.aggregator
            .endpoint_snapshot()
            .endpoints
            .iter()
            .filter_map(|e| e.p95_ms)
            .fold(0.0, f64::max)
    }
}

fn status_family_counts(status_counts: &HashMap<u16, u64>) -> HashMap<String, u64> {
    let mut out: HashMap<String, u64> = HashMap::new();
    for (code, count) in status_counts {
        let family = match *code {
            200..=299 => "2xx",
            300..=399 => "3xx",
            400..=499 => "4xx",
            _ => "5xx",
        };
        *out.entry(family.to_string()).or_insert(0) += *count;
    }
    out
}

fn percentile95(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((v.len() as f64) * 0.95).ceil() as usize;
    Some(v[idx.saturating_sub(1).min(v.len() - 1)])
}

fn dedupe_paths(paths: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for p in paths {
        if p == "off" || p.is_empty() {
            continue;
        }
        if seen.insert(p.clone()) {
            out.push(p.clone());
        }
    }
    out
}


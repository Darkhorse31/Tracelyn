use crate::events::payloads::{
    ApmEndpointRow, ApmHeatmapCell, ApmMetricsEvent, LiveLogLinePayload, MetricPointPayload,
    TimingNoticePayload, TopIpEntry, TopUrlEntry, WebTrafficEvent, WebTrafficMetricsPayload,
};
use crate::ssh::{run_command, start_tail_follow, SshSession};
use crate::webserver::{
    detect_and_parse_config, parse_access_line, suggest_apache_timing_line,
    suggest_nginx_timing_line, LogFormatKind, ParsedWebConfig, RouteNormalizer, RouteRule,
    TrafficAggregator,
};
use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::UnboundedReceiver;

const SERIES_LEN: usize = 60;

pub struct WebPipeline {
    aggregator: TrafficAggregator,
    log_format: LogFormatKind,
    timing_notice: Option<TimingNoticePayload>,
    permission_denied: bool,
    web_detected: bool,
    req_series: VecDeque<MetricPointPayload>,
    log_rx: Option<UnboundedReceiver<Result<String, String>>>,
}

impl WebPipeline {
    pub async fn setup(session: &SshSession) -> Self {
        let runner = crate::connection::ssh_runner::SessionRunner::new(session);
        let detected = detect_and_parse_config(&runner).await.ok().flatten();
        let mut permission_denied = false;
        let mut web_detected = false;
        let mut log_format = LogFormatKind::Combined;
        let mut timing_notice = None;
        let mut log_rx = None;

        if let Some(detected) = detected {
            web_detected = true;
            let (access_path, notice) = match &detected.config {
                Some(ParsedWebConfig::Nginx(info)) => {
                    if !info.request_timing.present {
                        timing_notice = Some(TimingNoticePayload {
                            message:
                                "Response time is not in the access log. Add a timed log_format and point access_log at it:"
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
                    log_format = LogFormatKind::Custom;
                    (
                        info.access_log_paths.first().cloned(),
                        timing_notice.clone(),
                    )
                }
                Some(ParsedWebConfig::Apache(info)) => {
                    if !info.request_timing.present {
                        timing_notice = Some(TimingNoticePayload {
                            message:
                                "Response time is not in the access log. Add %D (microseconds) to LogFormat:"
                                    .into(),
                            suggested_line: info
                                .request_timing
                                .suggested_line
                                .clone()
                                .unwrap_or_else(suggest_apache_timing_line),
                        });
                    }
                    log_format = LogFormatKind::Combined;
                    (info.access_log_paths.first().cloned(), timing_notice.clone())
                }
                None => (None, None),
            };
            timing_notice = notice;

            if let Some(path) = access_path {
                let readable = run_command(
                    session,
                    &format!("test -r '{}' && echo ok", path.replace('\'', "'\\''")),
                )
                .await
                .map(|s| s.trim() == "ok")
                .unwrap_or(false);
                if readable {
                    log_rx = start_tail_follow(session, &path).await.ok();
                } else {
                    permission_denied = true;
                }
            }
        }

        let normalizer = RouteNormalizer::new(Vec::<RouteRule>::new());
        Self {
            aggregator: TrafficAggregator::new(60, normalizer),
            log_format,
            timing_notice,
            permission_denied,
            web_detected,
            req_series: VecDeque::new(),
            log_rx,
        }
    }

    pub fn ingest_line(&mut self, line: &str) {
        if let Ok(entry) = parse_access_line(line, self.log_format) {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            self.aggregator.ingest(&entry, ts);
        }
    }

    pub fn drain_log_lines(&mut self) {
        let mut batch = Vec::new();
        if let Some(rx) = &mut self.log_rx {
            while let Ok(line) = rx.try_recv() {
                batch.push(line);
            }
        }
        for line in batch {
            match line {
                Ok(l) => self.ingest_line(&l),
                Err(msg) => {
                    if msg.to_lowercase().contains("permission denied") {
                        self.permission_denied = true;
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

        let heatmap: Vec<ApmHeatmapCell> = ep
            .heatmap
            .iter()
            .map(|c| ApmHeatmapCell {
                route: c.route.clone(),
                bucket_index: (c.bucket_secs / 60) as u32,
                p50_ms: c.p50_ms,
                p95_ms: c.p95_ms,
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

pub async fn redis_detected(session: &SshSession) -> bool {
    run_command(session, "ps aux 2>/dev/null || ps -ef")
        .await
        .map(|s| s.contains("redis-server"))
        .unwrap_or(false)
}

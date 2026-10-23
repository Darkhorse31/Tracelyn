use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricPointPayload {
    pub t: u64,
    pub v: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebTrafficMetricsPayload {
    pub req_per_sec: f64,
    pub bandwidth_mbps: f64,
    pub status_breakdown: HashMap<String, u64>,
    pub top_urls: Vec<TopUrlEntry>,
    pub top_ips: Vec<TopIpEntry>,
    pub req_per_sec_series: Vec<MetricPointPayload>,
    pub timing_notice: Option<TimingNoticePayload>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopUrlEntry {
    pub path: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopIpEntry {
    pub ip: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingNoticePayload {
    pub message: String,
    pub suggested_line: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveLogLinePayload {
    pub id: String,
    pub timestamp: u64,
    pub status: u16,
    pub method: String,
    pub path: String,
    pub remote_addr: String,
    pub latency_ms: Option<f64>,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebTrafficEvent {
    pub server_id: String,
    pub metrics: WebTrafficMetricsPayload,
    pub logs: Vec<LiveLogLinePayload>,
    pub permission_denied: bool,
    pub web_detected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApmEndpointRow {
    pub route: String,
    pub throughput: f64,
    pub avg_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub error_rate: f64,
    pub status_breakdown: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApmHeatmapCell {
    pub route: String,
    pub bucket_index: u32,
    pub p50_ms: f64,
    pub p95_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApmMetricsEvent {
    pub server_id: String,
    pub endpoints: Vec<ApmEndpointRow>,
    pub heatmap: Vec<ApmHeatmapCell>,
    pub timing_notice: Option<TimingNoticePayload>,
}

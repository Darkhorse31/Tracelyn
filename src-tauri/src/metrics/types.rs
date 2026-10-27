use serde::Serialize;

pub const SPARKLINE_LEN: usize = 60;
pub const CHART_HISTORY_LEN: usize = 120;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricPoint {
    pub t: u64,
    pub v: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewServerMetrics {
    pub server_id: String,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub disk_percent: f64,
    pub load_1: f64,
    pub cpu_sparkline: Vec<f64>,
    pub memory_sparkline: Vec<f64>,
    pub disk_sparkline: Vec<f64>,
    pub load_sparkline: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRow {
    pub pid: u32,
    pub user: String,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRow {
    pub name: String,
    pub active: bool,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SsSummary {
    pub tcp_total: Option<u64>,
    pub tcp_established: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerMetricsSnapshot {
    pub server_id: String,
    pub timestamp_ms: u64,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,
    pub disk_percent: f64,
    pub disk_read_bps: f64,
    pub disk_write_bps: f64,
    pub net_in_bps: f64,
    pub net_out_bps: f64,
    pub cpu_history: Vec<MetricPoint>,
    pub memory_history: Vec<MetricPoint>,
    pub load_history: Vec<MetricPoint>,
    pub disk_usage_history: Vec<MetricPoint>,
    pub disk_read_history: Vec<MetricPoint>,
    pub disk_write_history: Vec<MetricPoint>,
    pub net_in_history: Vec<MetricPoint>,
    pub net_out_history: Vec<MetricPoint>,
    pub processes: Vec<ProcessRow>,
    pub services: Vec<ServiceRow>,
    pub ss_summary: SsSummary,
    pub collector_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collector_name: Option<String>,
    /// Consecutive failed system polls. Zero means the last poll succeeded.
    #[serde(default)]
    pub collector_failures: u32,
}

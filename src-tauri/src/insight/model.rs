use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceSummary {
    pub trace_id: String,
    pub service: String,
    pub operation: String,
    pub start_ms: i64,
    pub duration_us: i64,
    pub status: String,
    pub span_count: i64,
    pub app_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanDetail {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service: String,
    pub operation: String,
    pub kind: String,
    pub status: String,
    pub start_ms: i64,
    pub duration_us: i64,
    pub app_id: Option<String>,
    pub peer: Option<String>,
    pub attributes: Value,
    pub events: Value,
    pub depth: u32,
    pub critical: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedLog {
    pub ts_ms: i64,
    pub severity: String,
    pub body: String,
    pub span_id: Option<String>,
    pub service: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceDetail {
    pub trace_id: String,
    pub spans: Vec<SpanDetail>,
    pub logs: Vec<RelatedLog>,
    pub error_fingerprints: Vec<String>,
    pub critical_path: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapNode {
    pub id: String,
    pub label: String,
    pub span_count: u64,
    pub error_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapEdge {
    pub source: String,
    pub target: String,
    pub requests: u64,
    pub errors: u64,
    pub rate_per_sec: f64,
    pub error_rate: f64,
    pub p95_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceMap {
    pub nodes: Vec<MapNode>,
    pub edges: Vec<MapEdge>,
    pub window_ms: i64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorIssue {
    pub fingerprint: String,
    pub app_id: Option<String>,
    pub title: String,
    pub language: String,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
    pub count: i64,
    pub state: String,
    pub sample_message: String,
    pub sample_stack: String,
    pub trace_ids: Vec<String>,
    pub sparkline: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlowQueryRow {
    pub engine: String,
    pub fingerprint: String,
    pub normalized: String,
    pub app_id: Option<String>,
    pub count: i64,
    pub total_ms: f64,
    pub avg_ms: f64,
    pub p95_ms: f64,
    pub rows_examined_avg: f64,
    pub last_seen_ms: i64,
    pub sample: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelSuggestion {
    pub path: String,
    pub app_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtelPathRow {
    pub app_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlowEngineStatus {
    pub engine: String,
    pub path: Option<String>,
    pub source: String,
    pub logging_on: bool,
    pub is_dir: bool,
    pub hint: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OtelStats {
    pub malformed: u64,
    pub spans_ingested: u64,
    pub files_ok: u32,
    pub files_error: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightSetup {
    pub paths: Vec<OtelPathRow>,
    pub suggestions: Vec<OtelSuggestion>,
    pub stats: OtelStats,
    pub engines: Vec<SlowEngineStatus>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceQuery {
    pub server_id: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub service: Option<String>,
    pub operation: Option<String>,
    pub status: Option<String>,
    pub min_duration_ms: Option<f64>,
    pub trace_id: Option<String>,
    pub app_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WatchedFile {
    pub path: String,
    pub app_id: String,
    pub kind: String,
    pub is_dir: bool,
}

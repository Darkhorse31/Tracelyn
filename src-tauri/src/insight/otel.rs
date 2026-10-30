use crate::insight::stack::{from_exception, fingerprint_text, ErrorDraft};
use serde_json::Value;

pub const MAX_SPANS: usize = 500;
pub const MAX_LOGS: usize = 200;
pub const MAX_METRICS: usize = 50;

#[derive(Debug, Clone)]
pub struct ParsedSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service: String,
    pub operation: String,
    pub kind: String,
    pub status: String,
    pub start_ms: i64,
    pub duration_us: i64,
    pub peer: Option<String>,
    pub attributes: Value,
    pub events: Value,
    pub app_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedLog {
    pub ts_ms: i64,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub service: String,
    pub severity: String,
    pub body: String,
    pub app_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedMetric {
    pub name: String,
    pub ts_ms: i64,
    pub value: f64,
}

#[derive(Debug, Default, Clone)]
pub struct OtelBatch {
    pub spans: Vec<ParsedSpan>,
    pub logs: Vec<ParsedLog>,
    pub metrics: Vec<ParsedMetric>,
    pub errors: Vec<ErrorDraft>,
    pub dropped: u64,
}

impl OtelBatch {
    fn is_empty(&self) -> bool {
        self.spans.is_empty() && self.logs.is_empty() && self.metrics.is_empty()
    }

    fn merge(&mut self, other: OtelBatch) {
        self.dropped += other.dropped;
        for span in other.spans {
            if self.spans.len() >= MAX_SPANS {
                self.dropped += 1;
            } else {
                self.spans.push(span);
            }
        }
        for log in other.logs {
            if self.logs.len() >= MAX_LOGS {
                self.dropped += 1;
            } else {
                self.logs.push(log);
            }
        }
        for metric in other.metrics {
            if self.metrics.len() >= MAX_METRICS {
                self.dropped += 1;
            } else {
                self.metrics.push(metric);
            }
        }
        self.errors.extend(other.errors);
    }
}

#[derive(Debug, Default)]
pub struct OtelAssembler {
    pub partial: String,
    pub malformed: u64,
}

impl OtelAssembler {
    pub fn push(&mut self, chunk: &str) -> OtelBatch {
        // ponytail: one OTLP export line can be the whole file. 2MB ceiling, raise if a single flush exceeds it.
        if self.partial.len() + chunk.len() > 2 * 1024 * 1024 && !self.partial.is_empty() {
            self.malformed += 1;
            self.partial.clear();
        }
        self.partial.push_str(chunk);
        let (docs, rest, garbage) = split_json_documents(&self.partial);
        self.malformed += garbage;
        self.partial = rest;
        if self.partial.len() > 2 * 1024 * 1024 {
            self.malformed += 1;
            self.partial.clear();
        }
        let mut batch = OtelBatch::default();
        for doc in docs {
            match serde_json::from_str::<Value>(&doc) {
                Ok(value) => {
                    let mut parsed = OtelBatch::default();
                    if !ingest_value(&value, &mut parsed) && parsed.is_empty() {
                        self.malformed += 1;
                    } else {
                        batch.merge(parsed);
                    }
                }
                Err(_) => self.malformed += 1,
            }
        }
        batch
    }
}

pub fn split_json_documents(input: &str) -> (Vec<String>, String, u64) {
    let bytes = input.as_bytes();
    let mut docs = Vec::new();
    let mut malformed = 0u64;
    let mut i = 0;
    let mut garbage = false;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c != b'{' && c != b'[' {
            garbage = true;
            i += 1;
            continue;
        }
        if garbage {
            malformed += 1;
            garbage = false;
        }
        match scan_value(bytes, i) {
            Some(end) => {
                docs.push(input[i..=end].to_string());
                i = end + 1;
            }
            None => return (docs, input[i..].to_string(), malformed),
        }
    }
    if garbage {
        malformed += 1;
    }
    (docs, String::new(), malformed)
}

fn scan_value(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    let mut i = start;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if escape {
                escape = false;
            } else if c == b'\\' {
                escape = true;
            } else if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn key<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| value.get(*name))
}

fn key_arr<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Vec<Value>> {
    key(value, names).and_then(|item| item.as_array())
}

fn key_str<'a>(value: &'a Value, names: &[&str]) -> Option<&'a str> {
    key(value, names).and_then(|item| item.as_str())
}

fn ingest_value(value: &Value, batch: &mut OtelBatch) -> bool {
    if let Some(items) = value.as_array() {
        let mut any = false;
        for item in items {
            any |= ingest_value(item, batch);
        }
        return any;
    }
    if !recognized(value) {
        return false;
    }
    if let Some(resources) = key_arr(value, &["resourceSpans", "resource_spans"]) {
        for resource in resources {
            let service = service_name(resource.get("resource"));
            let scopes = key_arr(
                resource,
                &["scopeSpans", "scope_spans", "instrumentationLibrarySpans", "instrumentation_library_spans"],
            );
            if let Some(scopes) = scopes {
                for scope in scopes {
                    if let Some(spans) = scope.get("spans").and_then(|v| v.as_array()) {
                        for span in spans {
                            push_span(batch, span, &service);
                        }
                    }
                }
            }
        }
    }
    if let Some(resources) = key_arr(value, &["resourceLogs", "resource_logs"]) {
        for resource in resources {
            let service = service_name(resource.get("resource"));
            let scopes = key_arr(
                resource,
                &["scopeLogs", "scope_logs", "instrumentationLibraryLogs", "instrumentation_library_logs"],
            );
            if let Some(scopes) = scopes {
                for scope in scopes {
                    if let Some(logs) = key_arr(scope, &["logRecords", "log_records"]) {
                        for log in logs {
                            push_log(batch, log, &service);
                        }
                    }
                }
            }
        }
    }
    if let Some(resources) = key_arr(value, &["resourceMetrics", "resource_metrics"]) {
        for resource in resources {
            let scopes = key_arr(
                resource,
                &["scopeMetrics", "scope_metrics", "instrumentationLibraryMetrics", "instrumentation_library_metrics"],
            );
            if let Some(scopes) = scopes {
                for scope in scopes {
                    if let Some(metrics) = scope.get("metrics").and_then(|v| v.as_array()) {
                        for metric in metrics {
                            push_metric(batch, metric);
                        }
                    }
                }
            }
        }
    }
    if key(value, &["traceId", "trace_id"]).is_some() && key(value, &["spanId", "span_id"]).is_some() {
        let service = service_name(Some(value));
        push_span(batch, value, &service);
    }
    true
}

fn recognized(value: &Value) -> bool {
    key(value, &["resourceSpans", "resource_spans"]).is_some()
        || key(value, &["resourceLogs", "resource_logs"]).is_some()
        || key(value, &["resourceMetrics", "resource_metrics"]).is_some()
        || (key(value, &["traceId", "trace_id"]).is_some() && key(value, &["spanId", "span_id"]).is_some())
}

fn push_span(batch: &mut OtelBatch, span: &Value, service: &str) {
    if batch.spans.len() >= MAX_SPANS {
        batch.dropped += 1;
        return;
    }
    let Some(trace_id) = key_str(span, &["traceId", "trace_id"]) else {
        return;
    };
    let Some(span_id) = key_str(span, &["spanId", "span_id"]) else {
        return;
    };
    if trace_id.is_empty() || span_id.is_empty() {
        return;
    }
    let attributes = flatten_attrs(span.get("attributes"));
    let service = attributes
        .get("service.name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(service)
        .to_string();
    let operation = span
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown");
    let start_ns = unix_nano(key(span, &["startTimeUnixNano", "start_time_unix_nano"])).unwrap_or(0);
    let end_ns = unix_nano(key(span, &["endTimeUnixNano", "end_time_unix_nano"])).unwrap_or(start_ns);
    let duration_us = if end_ns >= start_ns {
        ((end_ns - start_ns) / 1000) as i64
    } else {
        0
    };
    if let Some(arr) = span.get("events").and_then(|v| v.as_array()) {
        for event in arr {
            if event.get("name").and_then(|v| v.as_str()) == Some("exception") {
                let type_name = raw_string_attr(event, "exception.type");
                let message = raw_string_attr(event, "exception.message");
                let stack = raw_string_attr(event, "exception.stacktrace");
                let mut draft = from_exception(
                    &type_name,
                    &message,
                    &stack,
                    Some(trace_id.to_string()),
                    (start_ns / 1_000_000) as i64,
                );
                draft.app_id = None;
                batch.errors.push(draft);
            }
        }
    }
    let events = flatten_events(span.get("events"));
    batch.spans.push(ParsedSpan {
        trace_id: trace_id.to_string(),
        span_id: span_id.to_string(),
        parent_span_id: key_str(span, &["parentSpanId", "parent_span_id"])
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        service,
        operation: operation.to_string(),
        kind: span_kind(span.get("kind")),
        status: span_status(span),
        start_ms: (start_ns / 1_000_000) as i64,
        duration_us,
        peer: peer_from(&attributes),
        attributes,
        events,
        app_id: None,
    });
}

fn push_log(batch: &mut OtelBatch, log: &Value, service: &str) {
    if batch.logs.len() >= MAX_LOGS {
        batch.dropped += 1;
        return;
    }
    let body = log_body(log.get("body"));
    let severity = severity_of(log);
    let ts_ns = unix_nano(key(log, &["timeUnixNano", "time_unix_nano"]))
        .or_else(|| unix_nano(key(log, &["observedTimeUnixNano", "observed_time_unix_nano"])))
        .unwrap_or(0);
    let trace_id = key_str(log, &["traceId", "trace_id"])
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let span_id = key_str(log, &["spanId", "span_id"])
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    if is_error_severity(&severity) {
        if let Some(mut draft) = fingerprint_text(&body) {
            draft.trace_id = trace_id.clone();
            draft.ts_ms = (ts_ns / 1_000_000) as i64;
            batch.errors.push(draft);
        }
    }
    batch.logs.push(ParsedLog {
        ts_ms: (ts_ns / 1_000_000) as i64,
        trace_id,
        span_id,
        service: service.to_string(),
        severity,
        body: body.chars().take(2000).collect(),
        app_id: None,
    });
}

fn push_metric(batch: &mut OtelBatch, metric: &Value) {
    let name = metric
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("otel.metric");
    for metric_key in ["gauge", "sum"] {
        let Some(points) = metric
            .get(metric_key)
            .and_then(|v| key_arr(v, &["dataPoints", "data_points"]))
        else {
            continue;
        };
        for point in points {
            if batch.metrics.len() >= MAX_METRICS {
                batch.dropped += 1;
                return;
            }
            let Some(value) = key(point, &["asDouble", "as_double"])
                .and_then(json_f64)
                .or_else(|| key(point, &["asInt", "as_int"]).and_then(json_f64))
            else {
                continue;
            };
            let ts = unix_nano(key(point, &["timeUnixNano", "time_unix_nano"])).unwrap_or(0);
            batch.metrics.push(ParsedMetric {
                name: name.chars().take(80).collect(),
                ts_ms: (ts / 1_000_000) as i64,
                value,
            });
        }
    }
}

fn service_name(resource: Option<&Value>) -> String {
    resource
        .and_then(|r| attr_str(&flatten_attrs(r.get("attributes")), "service.name"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

fn flatten_attrs(value: Option<&Value>) -> Value {
    let mut map = serde_json::Map::new();
    let Some(value) = value else {
        return Value::Object(map);
    };
    if let Some(arr) = value.as_array() {
        for item in arr.iter().take(40) {
            let Some(key) = item.get("key").and_then(|k| k.as_str()) else {
                continue;
            };
            if key.is_empty() {
                continue;
            }
            let converted = item
                .get("value")
                .map(otel_any)
                .unwrap_or(Value::Null);
            map.insert(key.to_string(), bound_value(converted, 300));
        }
    } else if let Some(obj) = value.as_object() {
        for (key, item) in obj.iter().take(40) {
            map.insert(key.clone(), bound_value(item.clone(), 300));
        }
    }
    Value::Object(map)
}

fn flatten_events(value: Option<&Value>) -> Value {
    let Some(arr) = value.and_then(|v| v.as_array()) else {
        return Value::Array(Vec::new());
    };
    let events = arr
        .iter()
        .take(20)
        .map(|event| {
            let mut attrs = flatten_attrs(event.get("attributes"));
            if event.get("name").and_then(|v| v.as_str()) == Some("exception") {
                if let Some(obj) = attrs.as_object_mut() {
                    obj.insert(
                        "exception.stacktrace".into(),
                        Value::String(raw_string_attr(event, "exception.stacktrace")),
                    );
                }
            }
            serde_json::json!({
                "name": event.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                "timeMs": unix_nano(event.get("timeUnixNano")).unwrap_or(0) / 1_000_000,
                "attributes": attrs,
            })
        })
        .collect();
    Value::Array(events)
}

fn otel_any(value: &Value) -> Value {
    let Some(obj) = value.as_object() else {
        return value.clone();
    };
    if let Some(text) = obj.get("stringValue").or_else(|| obj.get("string_value")) {
        return text.clone();
    }
    if let Some(text) = obj
        .get("intValue")
        .or_else(|| obj.get("int_value"))
        .or_else(|| obj.get("doubleValue"))
        .or_else(|| obj.get("double_value"))
        .or_else(|| obj.get("boolValue"))
        .or_else(|| obj.get("bool_value"))
    {
        return text.clone();
    }
    if let Some(arr) = obj.get("arrayValue").and_then(|v| v.get("values")).and_then(|v| v.as_array()) {
        return Value::Array(arr.iter().take(8).map(otel_any).collect());
    }
    value.clone()
}

fn bound_value(value: Value, max: usize) -> Value {
    match value {
        Value::String(s) => Value::String(s.chars().take(max).collect()),
        other => other,
    }
}

fn raw_string_attr(event: &Value, key: &str) -> String {
    let Some(items) = event.get("attributes").and_then(|v| v.as_array()) else {
        return String::new();
    };
    for item in items {
        if item.get("key").and_then(|k| k.as_str()) != Some(key) {
            continue;
        }
        if let Some(text) = item
            .get("value")
            .and_then(|v| v.get("stringValue"))
            .and_then(|v| v.as_str())
        {
            return text.chars().take(8000).collect();
        }
    }
    String::new()
}

fn attr_str(attrs: &Value, key: &str) -> Option<String> {
    attrs
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn peer_from(attrs: &Value) -> Option<String> {
    if let Some(db) = attr_str(attrs, "db.system") {
        if let Some(name) = attr_str(attrs, "db.name") {
            return Some(format!("{db} {name}"));
        }
        return Some(db);
    }
    for key in [
        "peer.service",
        "net.peer.name",
        "server.address",
        "messaging.destination.name",
        "http.host",
        "net.peer.ip",
    ] {
        if let Some(value) = attr_str(attrs, key) {
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn span_kind(value: Option<&Value>) -> String {
    match value {
        Some(Value::Number(n)) => match n.as_u64() {
            Some(2) => "server",
            Some(3) => "client",
            Some(4) => "producer",
            Some(5) => "consumer",
            _ => "internal",
        },
        Some(Value::String(s)) => {
            let lower = s.to_ascii_lowercase();
            if lower.contains("server") {
                "server"
            } else if lower.contains("client") {
                "client"
            } else if lower.contains("producer") {
                "producer"
            } else if lower.contains("consumer") {
                "consumer"
            } else {
                "internal"
            }
        }
        _ => "internal",
    }
    .into()
}

fn span_status(span: &Value) -> String {
    match span.get("status").and_then(|s| s.get("code")) {
        Some(Value::Number(n)) if n.as_u64() == Some(2) => "error",
        Some(Value::Number(n)) if n.as_u64() == Some(1) => "ok",
        Some(Value::String(s)) if s.to_ascii_uppercase().contains("ERROR") => "error",
        Some(Value::String(s)) if s.to_ascii_uppercase().contains("OK") => "ok",
        _ => "unset",
    }
    .into()
}

fn log_body(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if let Some(text) = value.as_str() {
        return text.to_string();
    }
    if let Some(text) = key_str(value, &["stringValue", "string_value"]) {
        return text.to_string();
    }
    value.to_string()
}

fn severity_of(log: &Value) -> String {
    if let Some(text) = key_str(log, &["severityText", "severity_text"]) {
        return text.to_ascii_lowercase();
    }
    match key(log, &["severityNumber", "severity_number"]).and_then(|v| v.as_u64()) {
        Some(n) if n >= 21 => "fatal",
        Some(n) if n >= 17 => "error",
        Some(n) if n >= 13 => "warn",
        Some(n) if n >= 9 => "info",
        Some(n) if n >= 5 => "debug",
        Some(_) => "trace",
        None => "info",
    }
    .into()
}

fn is_error_severity(severity: &str) -> bool {
    let s = severity.to_ascii_lowercase();
    s.contains("error") || s.contains("fatal") || s.contains("critical")
}

fn unix_nano(value: Option<&Value>) -> Option<u128> {
    match value? {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_u64().map(|v| v as u128),
        _ => None,
    }
}

fn json_f64(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace_line() -> String {
        r#"{
          "resourceSpans": [{
            "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "api"}}]},
            "scopeSpans": [{
              "spans": [{
                "traceId": "abc123abc123abc123abc123abc123ab",
                "spanId": "def456def456def4",
                "parentSpanId": "",
                "name": "GET /users",
                "kind": 2,
                "startTimeUnixNano": "1700000000000000000",
                "endTimeUnixNano": "1700000000150000000",
                "attributes": [
                  {"key": "http.method", "value": {"stringValue": "GET"}},
                  {"key": "peer.service", "value": {"stringValue": "auth"}}
                ],
                "events": [{
                  "name": "exception",
                  "timeUnixNano": "1700000000100000000",
                  "attributes": [
                    {"key": "exception.type", "value": {"stringValue": "Error"}},
                    {"key": "exception.message", "value": {"stringValue": "boom"}},
                    {"key": "exception.stacktrace", "value": {"stringValue": "Error: boom\n    at handler (/app/src/index.js:20:15)"}}
                  ]
                }],
                "status": {"code": 2, "message": "boom"}
              }, {
                "traceId": "abc123abc123abc123abc123abc123ab",
                "spanId": "aaa456def456def4",
                "parentSpanId": "def456def456def4",
                "name": "SELECT",
                "kind": "SPAN_KIND_CLIENT",
                "startTimeUnixNano": "1700000000020000000",
                "endTimeUnixNano": "1700000000080000000",
                "attributes": [
                  {"key": "db.system", "value": {"stringValue": "mysql"}},
                  {"key": "db.name", "value": {"stringValue": "shop"}}
                ],
                "status": {"code": "STATUS_CODE_OK"}
              }]
            }]
          }]
        }"#
        .to_string()
    }

    #[test]
    fn parses_file_exporter_traces_logs_and_metrics() {
        let mut asm = OtelAssembler::default();
        let batch = asm.push(&trace_line());
        assert_eq!(batch.spans.len(), 2);
        assert_eq!(batch.spans[0].service, "api");
        assert_eq!(batch.spans[0].kind, "server");
        assert_eq!(batch.spans[0].status, "error");
        assert_eq!(batch.spans[0].duration_us, 150_000);
        assert_eq!(batch.spans[0].peer.as_deref(), Some("auth"));
        assert_eq!(batch.spans[1].kind, "client");
        assert_eq!(batch.spans[1].peer.as_deref(), Some("mysql shop"));
        assert_eq!(batch.spans[1].status, "ok");
        assert_eq!(batch.errors.len(), 1);
        assert_eq!(batch.errors[0].language, "node");
        assert_eq!(
            batch.errors[0].trace_id.as_deref(),
            Some("abc123abc123abc123abc123abc123ab")
        );

        let logs = r#"{"resourceLogs":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"api"}}]},"scopeLogs":[{"logRecords":[{"timeUnixNano":"1700000000200000000","severityText":"ERROR","body":{"stringValue":"Error: boom\n    at handler (/app/src/index.js:3:1)"},"traceId":"abc123abc123abc123abc123abc123ab","spanId":"def456def456def4"}]}]}]}"#;
        let logs = asm.push(logs);
        assert_eq!(logs.logs.len(), 1);
        assert_eq!(logs.logs[0].severity, "error");
        assert!(logs.logs[0].body.contains("boom"));
        assert_eq!(logs.errors.len(), 1);

        let metrics = r#"{"resourceMetrics":[{"scopeMetrics":[{"metrics":[{"name":"http.server.duration","sum":{"dataPoints":[{"asDouble":1.5,"timeUnixNano":"1700000000300000000"}]}}]}]}]}"#;
        let metrics = asm.push(metrics);
        assert_eq!(metrics.metrics.len(), 1);
        assert_eq!(metrics.metrics[0].name, "http.server.duration");
        assert!((metrics.metrics[0].value - 1.5).abs() < 0.001);
        assert_eq!(asm.malformed, 0);
    }

    #[test]
    fn partial_documents_malformed_lines_and_bare_spans() {
        let mut asm = OtelAssembler::default();
        let half = asm.push("{\"resourceSpans\":");
        assert!(half.spans.is_empty());
        assert!(!asm.partial.is_empty());
        let rest = asm.push("[{\"scopeSpans\":[{\"spans\":[]}]}]}");
        assert!(rest.spans.is_empty());
        assert!(asm.partial.is_empty());

        let bad = asm.push("\nthis is not json\n");
        assert!(bad.spans.is_empty());
        assert!(asm.malformed >= 1);

        let bare = asm.push(
            r#"{"traceId":"abc","spanId":"def","name":"work","kind":5,"startTimeUnixNano":"1000000","endTimeUnixNano":"2000000"}"#,
        );
        assert_eq!(bare.spans.len(), 1);
        assert_eq!(bare.spans[0].kind, "consumer");
        assert_eq!(bare.spans[0].service, "unknown");
    }

    #[test]
    fn mixed_otlp_json_lines_keep_service_name() {
        let trace = r#"{"resourceSpans":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"bharattales-web"}}]},"scopeSpans":[{"spans":[{"traceId":"4bf92f3577b34da6a3ce929d0e0e4736","spanId":"00f067aa0ba902b7","name":"GET /","kind":2,"startTimeUnixNano":"1700000000000000000","endTimeUnixNano":"1700000000120000000","attributes":[{"key":"http.request.method","value":{"stringValue":"GET"}}],"status":{"code":1}}]}]}]}"#;
        let log = r#"{"resourceLogs":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"bharattales-web"}}]},"scopeLogs":[{"logRecords":[{"timeUnixNano":"1700000000200000000","severityText":"INFO","body":{"stringValue":"request served"},"traceId":"4bf92f3577b34da6a3ce929d0e0e4736","spanId":"00f067aa0ba902b7"}]}]}]}"#;
        let mut asm = OtelAssembler::default();
        let batch = asm.push(&format!("{trace}\n{log}\n"));
        assert_eq!(batch.spans.len(), 1);
        assert_eq!(batch.spans[0].service, "bharattales-web");
        assert_eq!(batch.spans[0].operation, "GET /");
        assert_eq!(batch.spans[0].duration_us, 120_000);
        assert_eq!(batch.logs.len(), 1);
        assert_eq!(batch.logs[0].service, "bharattales-web");
        assert!(batch.logs[0].body.contains("request served"));
        assert_eq!(asm.malformed, 0);
    }

    #[test]
    fn file_exporter_fixture_parses_camel_and_snake() {
        let file = "\
{\"resourceSpans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"bharattales-web\"}},{\"key\":\"telemetry.sdk.language\",\"value\":{\"stringValue\":\"nodejs\"}}]},\"scopeSpans\":[{\"scope\":{\"name\":\"@opentelemetry/instrumentation-http\"},\"spans\":[{\"traceId\":\"4bf92f3577b34da6a3ce929d0e0e4736\",\"spanId\":\"00f067aa0ba902b7\",\"parentSpanId\":\"\",\"name\":\"GET /\",\"kind\":2,\"startTimeUnixNano\":\"1700000000000000000\",\"endTimeUnixNano\":\"1700000000120000000\",\"attributes\":[{\"key\":\"http.request.method\",\"value\":{\"stringValue\":\"GET\"}}],\"status\":{\"code\":1}}]}]}]}\n\
{\"resourceLogs\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"bharattales-web\"}}]},\"scopeLogs\":[{\"logRecords\":[{\"timeUnixNano\":\"1700000000200000000\",\"severityText\":\"INFO\",\"body\":{\"stringValue\":\"request served\"},\"traceId\":\"4bf92f3577b34da6a3ce929d0e0e4736\",\"spanId\":\"00f067aa0ba902b7\"}]}]}]}\n\
{\"resourceMetrics\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"stringValue\":\"bharattales-web\"}}]},\"scopeMetrics\":[{\"metrics\":[{\"name\":\"http.server.duration\",\"gauge\":{\"dataPoints\":[{\"asDouble\":12.5,\"timeUnixNano\":\"1700000000120000000\"}]}}]}]}]}\n\
{\"resource_spans\":[{\"resource\":{\"attributes\":[{\"key\":\"service.name\",\"value\":{\"string_value\":\"legacy\"}}]},\"scope_spans\":[{\"spans\":[{\"trace_id\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"span_id\":\"bbbbbbbbbbbbbbbb\",\"name\":\"old\",\"kind\":3,\"start_time_unix_nano\":\"1000\",\"end_time_unix_nano\":\"5000\"}]}]}]}\n";
        let mut asm = OtelAssembler::default();
        let batch = asm.push(file);
        assert_eq!(batch.spans.len(), 2);
        assert_eq!(batch.spans[0].service, "bharattales-web");
        assert_eq!(batch.spans[1].service, "legacy");
        assert_eq!(batch.spans[1].kind, "client");
        assert_eq!(batch.logs.len(), 1);
        assert_eq!(batch.metrics.len(), 1);
        assert_eq!(batch.metrics[0].value, 12.5);
        assert_eq!(asm.malformed, 0);
    }
}

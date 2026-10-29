//! Pull trace and span ids out of a log line.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceIds {
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

pub fn extract_trace_ids(line: &str) -> TraceIds {
    if let Some(found) = traceparent(line) {
        return found;
    }
    TraceIds {
        trace_id: first_field(line, &["trace_id", "traceId", "traceID"]),
        span_id: first_field(line, &["span_id", "spanId", "spanID"]),
    }
}

pub fn line_has_trace(line: &str, trace_id: &str) -> bool {
    if trace_id.len() < 8 {
        return false;
    }
    extract_trace_ids(line).trace_id.as_deref() == Some(trace_id)
}

fn traceparent(line: &str) -> Option<TraceIds> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 55 <= bytes.len() {
        if bytes[i] == b'0'
            && bytes.get(i + 1) == Some(&b'0')
            && bytes.get(i + 2) == Some(&b'-')
            && bytes.get(i + 35) == Some(&b'-')
            && bytes.get(i + 52) == Some(&b'-')
            && is_hex(&bytes[i + 3..i + 35])
            && is_hex(&bytes[i + 36..i + 52])
            && is_hex(&bytes[i + 53..i + 55])
        {
            return Some(TraceIds {
                trace_id: Some(line[i + 3..i + 35].to_string()),
                span_id: Some(line[i + 36..i + 52].to_string()),
            });
        }
        i += 1;
    }
    None
}

fn is_hex(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| b.is_ascii_hexdigit())
}

fn first_field(line: &str, names: &[&str]) -> Option<String> {
    for name in names {
        if let Some(value) = json_field(line, name).or_else(|| eq_field(line, name)) {
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn json_field(line: &str, name: &str) -> Option<String> {
    let quoted = format!("\"{name}\"");
    let idx = line.find(&quoted)?;
    let rest = line[idx + quoted.len()..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn eq_field(line: &str, name: &str) -> Option<String> {
    let token = format!("{name}=");
    let idx = line.find(&token)?;
    let rest = &line[idx + token.len()..];
    let value: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if value.is_empty() { None } else { Some(value) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_json_w3c_and_equals() {
        let json = r#"{"trace_id":"abc123def456","span_id":"span1"}"#;
        let ids = extract_trace_ids(json);
        assert_eq!(ids.trace_id.as_deref(), Some("abc123def456"));
        assert_eq!(ids.span_id.as_deref(), Some("span1"));

        let eq = "request trace_id=abc123def456 span_id=span9 done";
        let ids = extract_trace_ids(eq);
        assert_eq!(ids.trace_id.as_deref(), Some("abc123def456"));
        assert_eq!(ids.span_id.as_deref(), Some("span9"));

        let parent = "traceparent=00-0123456789abcdef0123456789abcdef-0123456789abcdef-01";
        let ids = extract_trace_ids(parent);
        assert_eq!(
            ids.trace_id.as_deref(),
            Some("0123456789abcdef0123456789abcdef")
        );
        assert_eq!(ids.span_id.as_deref(), Some("0123456789abcdef"));
        assert!(line_has_trace(parent, "0123456789abcdef0123456789abcdef"));
        assert!(!line_has_trace(json, "0123456789abcdef0123456789abcdef"));
    }
}

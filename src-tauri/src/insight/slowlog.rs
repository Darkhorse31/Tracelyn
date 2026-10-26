use crate::insight::fingerprint_hash;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct SlowDraft {
    pub engine: String,
    pub normalized: String,
    pub fingerprint: String,
    pub duration_ms: f64,
    pub rows_examined: i64,
    pub sample: String,
    pub ts_ms: i64,
    pub app_id: Option<String>,
}

pub fn normalize_statement(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\'' || c == b'"' || c == b'`' {
            i = skip_quoted(bytes, i);
            push_token(&mut out, "?");
            continue;
        }
        if c == b'0' && i + 1 < bytes.len() && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X') {
            let start = i;
            i += 2;
            while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                i += 1;
            }
            if i > start + 2 {
                push_token(&mut out, "?");
                continue;
            }
            i = start;
        }
        if c.is_ascii_digit() {
            let prev_ident = out
                .chars()
                .last()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_');
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if prev_ident {
                out.push_str(&input[start..i]);
            } else {
                push_token(&mut out, "?");
            }
            continue;
        }
        if c.is_ascii_whitespace() {
            if !out.ends_with(' ') && !out.is_empty() {
                out.push(' ');
            }
            i += 1;
            continue;
        }
        out.push((c as char).to_ascii_lowercase());
        i += 1;
    }
    tidy_sql(out.trim())
}

fn tidy_sql(input: &str) -> String {
    let input = input.trim().trim_end_matches(';').trim();
    let mut spaced = String::new();
    for c in input.chars() {
        if matches!(c, ',' | '=' | '(' | ')') {
            if !spaced.ends_with(' ') && !spaced.is_empty() {
                spaced.push(' ');
            }
            spaced.push(c);
            spaced.push(' ');
        } else {
            spaced.push(c);
        }
    }
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn push_token(out: &mut String, token: &str) {
    if out.ends_with(' ') || out.is_empty() {
        out.push_str(token);
    } else if out
        .chars()
        .last()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '?')
    {
        out.push(' ');
        out.push_str(token);
    } else {
        out.push_str(token);
    }
}

fn skip_quoted(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            if quote != b'\'' && i + 1 < bytes.len() && bytes[i + 1] == quote {
                i += 2;
                continue;
            }
            if quote == b'\'' && i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

pub fn normalize_mongo_command(command: &Value) -> String {
    let mut copy = command.clone();
    scrub_mongo(&mut copy);
    serde_json::to_string(&copy).unwrap_or_else(|_| "mongo".into())
}

fn scrub_mongo(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                let keep_collection = matches!(
                    key.as_str(),
                    "find" | "insert" | "update" | "delete" | "aggregate" | "count" | "distinct"
                );
                if let Some(child) = map.get_mut(&key) {
                    if keep_collection && child.is_string() {
                        continue;
                    }
                    scrub_mongo(child);
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                scrub_mongo(child);
            }
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) => {
            *value = Value::String("?".into());
        }
        _ => {}
    }
}

fn draft(engine: &str, sql: &str, duration_ms: f64, rows: i64, ts_ms: i64) -> SlowDraft {
    let normalized = if engine == "mongo" {
        sql.to_string()
    } else {
        normalize_statement(sql)
    };
    let sample = sql.chars().take(500).collect();
    SlowDraft {
        fingerprint: fingerprint_hash(&format!("{engine}\n{normalized}")),
        engine: engine.into(),
        normalized,
        duration_ms,
        rows_examined: rows,
        sample,
        ts_ms,
        app_id: None,
    }
}

pub fn parse_mysql_entry(block: &str) -> Option<SlowDraft> {
    let mut duration = None;
    let mut rows = 0i64;
    let mut sql = String::new();
    let mut ts = 0i64;
    for line in block.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(rest) = t.strip_prefix("# Time:") {
            ts = parse_mysql_time(rest);
            continue;
        }
        if t.contains("Query_time:") {
            duration = parse_query_time(t);
            rows = parse_rows_examined(t);
            continue;
        }
        if t.starts_with('#') {
            continue;
        }
        let lower = t.to_ascii_lowercase();
        if lower.starts_with("set timestamp=") || lower.starts_with("use ") {
            continue;
        }
        if !sql.is_empty() {
            sql.push(' ');
        }
        sql.push_str(t);
    }
    let duration = duration?;
    if sql.is_empty() {
        return None;
    }
    Some(draft("mysql", &sql, duration * 1000.0, rows, ts))
}

pub fn push_mysql(buf: &mut String, chunk: &str) -> (Vec<SlowDraft>, u64) {
    let mut malformed = 0u64;
    buf.push_str(chunk);
    if buf.len() > 64 * 1024 {
        malformed += 1;
        buf.clear();
        return (Vec::new(), malformed);
    }
    let mut out = Vec::new();
    loop {
        let Some(idx) = find_second_header(buf) else {
            break;
        };
        let entry: String = buf.drain(..idx).collect();
        match parse_mysql_entry(&entry) {
            Some(row) => out.push(row),
            None if entry.contains("Query_time:") => malformed += 1,
            None => {}
        }
    }
    if mysql_ready(buf) {
        if let Some(row) = parse_mysql_entry(buf) {
            out.push(row);
            buf.clear();
        }
    }
    (out, malformed)
}

fn mysql_ready(buf: &str) -> bool {
    if !buf.ends_with('\n') || parse_mysql_entry(buf).is_none() {
        return false;
    }
    buf.ends_with("\n\n")
        || buf.lines().any(|line| {
            let t = line.trim();
            !t.is_empty() && !t.starts_with('#') && t.contains(';')
        })
}

fn find_second_header(buf: &str) -> Option<usize> {
    let mut seen_time = false;
    let mut seen_user = false;
    let mut offset = 0;
    while offset < buf.len() {
        let rest = &buf[offset..];
        let line_start = offset == 0 || buf.as_bytes()[offset - 1] == b'\n';
        if line_start && rest.starts_with("# Time:") {
            if seen_time || seen_user {
                return Some(offset);
            }
            seen_time = true;
        } else if line_start && rest.starts_with("# User@Host:") {
            if seen_user {
                return Some(offset);
            }
            seen_user = true;
        }
        offset += rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    None
}

fn parse_mysql_time(rest: &str) -> i64 {
    let s = rest.trim();
    if let Some(ms) = parse_rfc3339_ms(s) {
        return ms;
    }
    if s.len() >= 15 && s.as_bytes().get(6) == Some(&b' ') {
        let Ok(yy) = s[0..2].parse::<i32>() else {
            return 0;
        };
        let year = if yy >= 70 { 1900 + yy } else { 2000 + yy };
        let stamp = format!("{year:04}-{}-{}T{}Z", &s[2..4], &s[4..6], &s[7..15]);
        return parse_rfc3339_ms(&stamp).unwrap_or(0);
    }
    0
}

pub fn parse_postgres_line(line: &str) -> Option<SlowDraft> {
    let lower = line.to_ascii_lowercase();
    let idx = lower.find("duration:")?;
    let after = line[idx + "duration:".len()..].trim();
    let (num, rest) = take_number(after)?;
    let rest = rest.trim();
    let rest_lower = rest.to_ascii_lowercase();
    if !rest_lower.starts_with("ms") {
        return None;
    }
    let rest = rest[2..].trim();
    let rest_lower = rest.to_ascii_lowercase();
    let sql = if let Some(i) = rest_lower.find("statement:") {
        rest[i + "statement:".len()..].trim()
    } else if let Some(i) = rest_lower.find("execute ") {
        let tail = &rest[i + "execute ".len()..];
        tail.split_once(':').map(|(_, sql)| sql.trim())?
    } else {
        return None;
    };
    if sql.is_empty() {
        return None;
    }
    let ts = parse_pg_prefix_ms(line).unwrap_or(0);
    Some(draft("postgres", sql, num, 0, ts))
}

pub fn parse_mongo_line(line: &str) -> Option<SlowDraft> {
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    let msg = value.get("msg").and_then(|m| m.as_str()).unwrap_or("");
    if !msg.to_ascii_lowercase().contains("slow") {
        return None;
    }
    let attr = value.get("attr")?;
    let duration = json_f64(attr.get("durationMillis")?)?;
    let command = attr.get("command").cloned().unwrap_or(Value::Null);
    let normalized = normalize_mongo_command(&command);
    let rows = attr
        .get("docsExamined")
        .or_else(|| attr.get("keysExamined"))
        .and_then(json_i64)
        .unwrap_or(0);
    let ts = value
        .get("t")
        .and_then(|t| t.get("$date"))
        .and_then(|d| d.as_str())
        .and_then(parse_rfc3339_ms)
        .unwrap_or(0);
    let sample = serde_json::to_string(&command).unwrap_or_default();
    Some(SlowDraft {
        fingerprint: fingerprint_hash(&format!("mongo\n{normalized}")),
        engine: "mongo".into(),
        normalized,
        duration_ms: duration,
        rows_examined: rows,
        sample: sample.chars().take(500).collect(),
        ts_ms: ts,
        app_id: None,
    })
}

pub fn push_text_lines(partial: &mut String, chunk: &str) -> (Vec<String>, u64) {
    let mut malformed = 0u64;
    partial.push_str(chunk);
    let mut lines = Vec::new();
    while let Some(idx) = partial.find('\n') {
        let mut line: String = partial.drain(..=idx).collect();
        if line.ends_with('\n') {
            line.pop();
        }
        if line.ends_with('\r') {
            line.pop();
        }
        lines.push(line);
    }
    if partial.len() > 64 * 1024 {
        malformed += 1;
        partial.clear();
    }
    (lines, malformed)
}

fn parse_query_time(line: &str) -> Option<f64> {
    let lower = line.to_ascii_lowercase();
    let idx = lower.find("query_time:")?;
    let after = line[idx + "query_time:".len()..].trim();
    take_number(after).map(|(n, _)| n)
}

fn parse_rows_examined(line: &str) -> i64 {
    let lower = line.to_ascii_lowercase();
    lower
        .find("rows_examined:")
        .and_then(|idx| take_number(line[idx + "rows_examined:".len()..].trim()))
        .map(|(n, _)| n as i64)
        .unwrap_or(0)
}

fn take_number(input: &str) -> Option<(f64, &str)> {
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let n = input[..i].parse().ok()?;
    Some((n, &input[i..]))
}

fn json_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|n| n as f64))
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

fn json_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().map(|n| n as i64))
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

pub fn parse_rfc3339_ms(input: &str) -> Option<i64> {
    let s = input.trim().trim_matches(|c| c == '\'' || c == '"');
    if s.len() < 19 {
        return None;
    }
    let y: i32 = s[0..4].parse().ok()?;
    let mo: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    let h: u32 = s[11..13].parse().ok()?;
    let mi: u32 = s[14..16].parse().ok()?;
    let sec: u32 = s[17..19].parse().ok()?;
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let days = days_from_civil(y, mo as i32, d as i32);
    let secs = days * 86_400 + i64::from(h) * 3600 + i64::from(mi) * 60 + i64::from(sec);
    Some(secs.saturating_mul(1000))
}

fn parse_pg_prefix_ms(line: &str) -> Option<i64> {
    let date = line.get(0..10)?;
    let time = line.get(11..19)?;
    parse_rfc3339_ms(&format!("{date}T{time}Z"))
}

fn days_from_civil(mut y: i32, m: i32, d: i32) -> i64 {
    y -= i32::from(m <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u64;
    era as i64 * 146_097 + doe as i64 - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_collapse_to_the_same_fingerprint() {
        let a = normalize_statement("SELECT * FROM orders WHERE user_id = 42 AND status = 'open'");
        let b = normalize_statement("select * from orders where user_id=7 and status='closed'");
        assert_eq!(a, b);
        assert_eq!(a, "select * from orders where user_id = ? and status = ?");
        assert_eq!(
            fingerprint_hash(&format!("mysql\n{a}")),
            fingerprint_hash(&format!("mysql\n{b}"))
        );
        assert!(normalize_statement("SELECT * FROM col2 WHERE id = 1").contains("col2"));
    }

    #[test]
    fn mysql_slow_log_entry() {
        let block = "\
# Time: 2026-10-26T10:00:00.000000Z
# User@Host: app[app] @ localhost []  Id: 12
# Query_time: 2.345678  Lock_time: 0.000012 Rows_sent: 10  Rows_examined: 100000
SET timestamp=1761472800;
SELECT * FROM orders WHERE user_id = 42 AND status = 'open';
";
        let (rows, malformed) = push_mysql(&mut String::new(), block);
        assert_eq!(malformed, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].engine, "mysql");
        assert!((rows[0].duration_ms - 2345.678).abs() < 0.01);
        assert_eq!(rows[0].rows_examined, 100000);
        assert!(rows[0].normalized.contains("user_id = ?"));
        assert!(rows[0].ts_ms > 0);
        let again = normalize_statement("SELECT * FROM orders WHERE user_id = 1 AND status = 'x'");
        assert_eq!(rows[0].normalized, again);
    }

    #[test]
    fn postgres_duration_line() {
        let line = "2026-10-26 10:00:01.123 UTC [12345] user@db LOG:  duration: 345.678 ms  statement: SELECT * FROM orders WHERE user_id = 42";
        let row = parse_postgres_line(line).unwrap();
        assert_eq!(row.engine, "postgres");
        assert!((row.duration_ms - 345.678).abs() < 0.001);
        assert!(row.normalized.starts_with("select * from orders"));
        let exec = "duration: 12.5 ms  execute <unnamed>: SELECT 1";
        assert!(parse_postgres_line(exec).unwrap().normalized.contains("select ?"));
    }

    #[test]
    fn mongo_slow_json() {
        let line = r#"{"t":{"$date":"2026-10-26T10:00:00.000+00:00"},"s":"I","c":"COMMAND","msg":"Slow query","attr":{"type":"command","ns":"shop.orders","command":{"find":"orders","filter":{"userId":42}},"docsExamined":5000,"nreturned":10,"durationMillis":250}}"#;
        let row = parse_mongo_line(line).unwrap();
        assert_eq!(row.engine, "mongo");
        assert!((row.duration_ms - 250.0).abs() < 0.01);
        assert_eq!(row.rows_examined, 5000);
        assert!(row.normalized.contains("\"find\":\"orders\""));
        assert!(row.normalized.contains("\"userId\":\"?\""));
        let other = r#"{"msg":"Slow query","attr":{"durationMillis":10,"command":{"find":"orders","filter":{"userId":7}},"docsExamined":1}}"#;
        assert_eq!(row.fingerprint, parse_mongo_line(other).unwrap().fingerprint);
        assert!(parse_mongo_line(r#"{"msg":"Connection accepted","attr":{"durationMillis":1}}"#).is_none());
    }

    #[test]
    fn known_unix_day() {
        assert_eq!(parse_rfc3339_ms("2024-01-01T00:00:00Z"), Some(1_704_067_200_000));
    }
}

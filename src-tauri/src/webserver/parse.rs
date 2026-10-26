use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogFormatKind {
    Common,
    Combined,
    Custom,
    ApacheTiming,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessLogEntry {
    pub remote_addr: String,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub status: u16,
    pub bytes_sent: u64,
    pub request_time_secs: Option<f64>,
    pub upstream_time_secs: Option<f64>,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub raw_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
}

impl ParseError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

/// Parse nginx/apache combined/common lines and custom nginx-style quoted fields.
pub fn parse_access_line(line: &str, format: LogFormatKind) -> Result<AccessLogEntry, ParseError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(ParseError::new("empty line"));
    }
    match format {
        LogFormatKind::Combined => parse_combined(trimmed),
        LogFormatKind::Common => parse_common(trimmed),
        LogFormatKind::Custom => parse_nginx_extended(trimmed),
        LogFormatKind::ApacheTiming => parse_apache_timing_line(trimmed),
    }
}

fn parse_common(line: &str) -> Result<AccessLogEntry, ParseError> {
    parse_combined_inner(line, false)
}

fn parse_combined(line: &str) -> Result<AccessLogEntry, ParseError> {
    parse_combined_inner(line, true)
}

fn parse_combined_inner(line: &str, with_extras: bool) -> Result<AccessLogEntry, ParseError> {
    let (ip, rest) = take_token(line).ok_or_else(|| ParseError::new("missing ip"))?;
    let rest = skip_sep(rest);
    let (_dash, rest) = take_token(rest).ok_or_else(|| ParseError::new("missing ident"))?;
    let rest = skip_sep(rest);
    let (_user, rest) = take_token(rest).ok_or_else(|| ParseError::new("missing user"))?;
    let rest = skip_sep(rest);
    let (time_part, rest) = take_bracketed(rest).ok_or_else(|| ParseError::new("missing time"))?;
    if time_part.is_empty() {
        return Err(ParseError::new("empty time"));
    }
    let rest = skip_sep(rest);
    let (request, rest) = take_quoted_required(rest).ok_or_else(|| ParseError::new("missing request"))?;
    let (method, path, query) = parse_request_line(&request)?;
    let rest = skip_sep(rest);
    let (status_s, rest) = take_token(rest).ok_or_else(|| ParseError::new("missing status"))?;
    let status = status_s.parse().map_err(|_| ParseError::new("bad status"))?;
    let rest = skip_sep(rest);
    let (bytes_s, rest) = take_token(rest).ok_or_else(|| ParseError::new("missing bytes"))?;
    let bytes_sent = parse_bytes(&bytes_s)?;

    let (referer, user_agent) = if with_extras {
        let (r, r2) = take_quoted(rest).unwrap_or((None, rest));
        let referer = r.filter(|s| s != "-");
        let rest = skip_sep(r2);
        let (ua, _r3) = take_quoted(rest).unwrap_or((None, rest));
        let user_agent = ua.filter(|s| s != "-");
        (referer, user_agent)
    } else {
        (None, None)
    };

    Ok(AccessLogEntry {
        remote_addr: ip,
        method,
        path,
        query,
        status,
        bytes_sent,
        request_time_secs: None,
        upstream_time_secs: None,
        user_agent,
        referer,
        raw_line: line.to_string(),
    })
}

fn parse_nginx_extended(line: &str) -> Result<AccessLogEntry, ParseError> {
    let mut entry = parse_combined(line)?;
    let tail = line.rsplitn(3, ' ').collect::<Vec<_>>();
    if tail.len() >= 2 {
        if let Ok(t) = tail[0].parse::<f64>() {
            entry.upstream_time_secs = Some(t);
        }
        if let Ok(t) = tail[1].parse::<f64>() {
            entry.request_time_secs = Some(t);
        }
    }
    // More reliable: scan trailing numeric fields after last quote
    if entry.request_time_secs.is_none() {
        if let Some(idx) = line.rfind('"') {
            let after = line[idx + 1..].trim();
            let nums: Vec<f64> = after
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            if nums.len() >= 1 {
                entry.request_time_secs = Some(nums[0]);
            }
            if nums.len() >= 2 {
                entry.upstream_time_secs = Some(nums[1]);
            }
        }
    }
    Ok(entry)
}

/// Apache %D is microseconds in the last field when using custom combined_timing format.
pub fn parse_apache_timing_line(line: &str) -> Result<AccessLogEntry, ParseError> {
    let entry = parse_combined(line)?;
    let micros = line
        .split_whitespace()
        .last()
        .and_then(|s| s.parse::<u64>().ok());
    let secs = micros.map(|m| m as f64 / 1_000_000.0);
    Ok(AccessLogEntry {
        request_time_secs: secs,
        upstream_time_secs: None,
        ..entry
    })
}

fn parse_bytes(s: &str) -> Result<u64, ParseError> {
    if s == "-" {
        return Ok(0);
    }
    s.parse().map_err(|_| ParseError::new("bad bytes"))
}

fn parse_request_line(req: &str) -> Result<(String, String, Option<String>), ParseError> {
    let parts: Vec<&str> = req.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(ParseError::new("bad request line"));
    }
    let method = parts[0].to_string();
    let uri = parts[1];
    let (path, query) = match uri.split_once('?') {
        Some((p, q)) => (p.to_string(), Some(q.to_string())),
        None => (uri.to_string(), None),
    };
    Ok((method, path, query))
}

fn take_token(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    if s.starts_with('"') {
        return take_quoted_required(s).map(|(v, r)| (v, r));
    }
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    Some((s[..end].to_string(), s[end..].trim_start()))
}

fn take_bracketed(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if !s.starts_with('[') {
        return None;
    }
    let end = s.find(']')?;
    let inner = s[1..end].to_string();
    Some((inner, s[end + 1..].trim_start()))
}

fn take_quoted_required(s: &str) -> Option<(String, &str)> {
    take_quoted(s).and_then(|(inner, rest)| inner.map(|v| (v, rest)))
}

fn take_quoted(s: &str) -> Option<(Option<String>, &str)> {
    let s = s.trim_start();
    if !s.starts_with('"') {
        return None;
    }
    let mut i = 1;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'"' && bytes[i - 1] != b'\\' {
            break;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let inner = s[1..i].to_string();
    Some((Some(inner), s[i + 1..].trim_start()))
}

fn skip_sep(s: &str) -> &str {
    s.trim_start()
}
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
    Unknown,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Fatal => "fatal",
            Self::Unknown => "unknown",
        }
    }
}

pub fn detect_level(line: &str) -> LogLevel {
    infer_level(line, None)
}

/// `stream` is `"stderr"` when the line came from a process stderr source.
pub fn infer_level(line: &str, stream: Option<&str>) -> LogLevel {
    if line.trim().is_empty() {
        return LogLevel::Unknown;
    }
    if let Some(level) = json_level(line) {
        return level;
    }
    if let Some(level) = syslog_pri(line) {
        return level;
    }
    if let Some(level) = keyword_level(line) {
        return level;
    }
    if let Some(level) = http_status_level(line) {
        return level;
    }
    if stream == Some("stderr") {
        return LogLevel::Error;
    }
    LogLevel::Unknown
}

pub fn stream_from_source(source_id: &str) -> Option<&'static str> {
    if source_id == "stderr" || source_id.ends_with("-stderr") || source_id.ends_with(":stderr") {
        Some("stderr")
    } else {
        None
    }
}

fn json_level(line: &str) -> Option<LogLevel> {
    let v = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let obj = v.as_object()?;
    for key in ["level", "severity", "lvl", "levelname"] {
        let Some(value) = obj.get(key) else {
            continue;
        };
        if let Some(l) = value.as_str() {
            if let Ok(n) = l.parse::<u64>() {
                if let Some(level) = numeric_level(n) {
                    return Some(level);
                }
            }
            let parsed = parse_level_str(l);
            if parsed != LogLevel::Unknown {
                return Some(parsed);
            }
        }
        if let Some(n) = value.as_u64() {
            if let Some(level) = numeric_level(n) {
                return Some(level);
            }
        }
        if let Some(n) = value.as_i64() {
            if n >= 0 {
                if let Some(level) = numeric_level(n as u64) {
                    return Some(level);
                }
            }
        }
    }
    None
}

fn syslog_pri(line: &str) -> Option<LogLevel> {
    let rest = line.trim_start().strip_prefix('<')?;
    let (num, _) = rest.split_once('>')?;
    if num.is_empty() || num.len() > 3 || !num.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let pri: u64 = num.parse().ok()?;
    if pri > 191 {
        return None;
    }
    numeric_level(pri % 8)
}

fn keyword_level(line: &str) -> Option<LogLevel> {
    let mut best: Option<LogLevel> = None;
    for raw in line.split(|c: char| !c.is_ascii_alphanumeric()) {
        if raw.is_empty() {
            continue;
        }
        let level = match raw.to_ascii_uppercase().as_str() {
            "FATAL" | "EMERG" | "EMERGENCY" | "ALERT" | "CRIT" | "CRITICAL" | "PANIC" => {
                LogLevel::Fatal
            }
            "ERROR" | "ERR" | "EXCEPTION" | "TRACEBACK" => LogLevel::Error,
            "WARN" | "WARNING" => LogLevel::Warn,
            "NOTICE" | "INFO" => LogLevel::Info,
            "DEBUG" => LogLevel::Debug,
            "TRACE" => LogLevel::Trace,
            _ => continue,
        };
        best = Some(match best {
            Some(prev) if rank(prev) >= rank(level) => prev,
            _ => level,
        });
    }
    best
}

fn rank(level: LogLevel) -> u8 {
    match level {
        LogLevel::Fatal => 6,
        LogLevel::Error => 5,
        LogLevel::Warn => 4,
        LogLevel::Info => 3,
        LogLevel::Debug => 2,
        LogLevel::Trace => 1,
        LogLevel::Unknown => 0,
    }
}

fn http_status_level(line: &str) -> Option<LogLevel> {
    let bytes = line.as_bytes();
    let marker = b"HTTP/";
    let mut i = 0;
    while i + marker.len() < bytes.len() {
        if bytes[i..].starts_with(marker) {
            let after = &line[i + marker.len()..];
            let rest = after.split('"').nth(1).unwrap_or("");
            let status: u16 = rest.split_whitespace().next()?.parse().ok()?;
            return status_level(status);
        }
        i += 1;
    }
    None
}

fn status_level(status: u16) -> Option<LogLevel> {
    if (500..600).contains(&status) {
        Some(LogLevel::Error)
    } else if (400..500).contains(&status) {
        Some(LogLevel::Warn)
    } else {
        None
    }
}

fn numeric_level(n: u64) -> Option<LogLevel> {
    if n <= 7 {
        return Some(match n {
            0 | 1 | 2 => LogLevel::Fatal,
            3 => LogLevel::Error,
            4 => LogLevel::Warn,
            5 | 6 => LogLevel::Info,
            _ => LogLevel::Debug,
        });
    }
    Some(match n {
        10 => LogLevel::Trace,
        20 => LogLevel::Debug,
        30 => LogLevel::Info,
        40 => LogLevel::Warn,
        50 => LogLevel::Error,
        60 => LogLevel::Fatal,
        _ => return None,
    })
}

fn parse_level_str(s: &str) -> LogLevel {
    match s.to_lowercase().as_str() {
        "trace" => LogLevel::Trace,
        "debug" => LogLevel::Debug,
        "info" | "information" | "notice" => LogLevel::Info,
        "warn" | "warning" => LogLevel::Warn,
        "error" | "err" | "exception" | "panic" => LogLevel::Error,
        "fatal" | "critical" | "crit" | "alert" | "emerg" | "emergency" => LogLevel::Fatal,
        _ => LogLevel::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_and_plain_levels() {
        assert_eq!(detect_level(r#"{"level":"error","msg":"nope"}"#), LogLevel::Error);
        assert_eq!(detect_level(r#"{"level":30,"msg":"ok"}"#), LogLevel::Info);
        assert_eq!(detect_level(r#"{"severity":"CRITICAL"}"#), LogLevel::Fatal);
        assert_eq!(detect_level(r#"{"severity":3,"msg":"down"}"#), LogLevel::Error);
        assert_eq!(detect_level("ERROR failed to bind"), LogLevel::Error);
        assert_eq!(
            detect_level("Traceback (most recent call last):"),
            LogLevel::Error
        );
    }

    #[test]
    fn keywords_syslog_http_and_stderr() {
        assert_eq!(detect_level("[ERR] disk full"), LogLevel::Error);
        assert_eq!(detect_level("FATAL: abort"), LogLevel::Fatal);
        assert_eq!(detect_level("PANIC rust"), LogLevel::Fatal);
        assert_eq!(detect_level("WARN slow query"), LogLevel::Warn);
        assert_eq!(detect_level("NOTICE ready"), LogLevel::Info);
        assert_eq!(detect_level("DEBUG handshake"), LogLevel::Debug);
        assert_eq!(detect_level("Exception in thread main"), LogLevel::Error);
        assert_eq!(detect_level("<11>sshd bad auth"), LogLevel::Error);
        assert_eq!(detect_level("<14>cron started"), LogLevel::Info);
        assert_eq!(
            detect_level(r#"127.0.0.1 - - [26/Oct/2026] "GET / HTTP/1.1" 500 12"#),
            LogLevel::Error
        );
        assert_eq!(
            detect_level(r#"127.0.0.1 - - [26/Oct/2026] "GET /no HTTP/1.1" 404 9"#),
            LogLevel::Warn
        );
        assert_eq!(detect_level("nothing to see here"), LogLevel::Unknown);
        assert_eq!(infer_level("nothing to see here", Some("stderr")), LogLevel::Error);
        assert_eq!(infer_level("INFO still info", Some("stderr")), LogLevel::Info);
        assert_eq!(infer_level("WARNING careful", Some("stderr")), LogLevel::Warn);
    }
}

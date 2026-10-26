use crate::logs::level::{detect_level, infer_level, stream_from_source, LogLevel};
use serde::Serialize;
use std::collections::VecDeque;

pub const RING_CAPACITY: usize = 100_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub id: String,
    pub ts_ms: u64,
    pub app_id: Option<String>,
    pub source_id: String,
    pub level: LogLevel,
    pub message: String,
    pub raw: String,
    pub http_status: Option<u16>,
    pub multiline_group_id: Option<String>,
}

pub struct LogRingBuffer {
    lines: VecDeque<LogLine>,
    next_id: u64,
    emitted: u64,
}

impl LogRingBuffer {
    pub fn new() -> Self {
        Self {
            lines: VecDeque::new(),
            next_id: 1,
            emitted: 0,
        }
    }

    pub fn push(&mut self, source_id: &str, app_id: Option<&str>, raw: &str) {
        let id_num = self.next_id;
        self.next_id += 1;
        let id = format!("log-{id_num}");
        let level = match stream_from_source(source_id) {
            Some(stream) => infer_level(raw, Some(stream)),
            None => detect_level(raw),
        };
        let continuation = is_continuation(raw);
        let multiline_group_id = if continuation {
            self.lines.back_mut().and_then(|prev| {
                if prev.multiline_group_id.is_none() {
                    prev.multiline_group_id = Some(prev.id.clone());
                }
                prev.multiline_group_id.clone()
            })
        } else if starts_group(raw) {
            Some(id.clone())
        } else {
            None
        };
        let message = raw.chars().take(500).collect();
        self.lines.push_back(LogLine {
            id,
            ts_ms: chrono_now_ms(),
            app_id: app_id.map(|s| s.to_string()),
            source_id: source_id.to_string(),
            level,
            message,
            raw: raw.to_string(),
            http_status: extract_http_status(raw),
            multiline_group_id,
        });
        while self.lines.len() > RING_CAPACITY {
            self.lines.pop_front();
        }
    }

    /// Lines not yet shipped to the UI, capped so one tick cannot serialize the ring.
    pub fn drain_new(&mut self, max: usize) -> Vec<LogLine> {
        let mut out = Vec::new();
        for line in &self.lines {
            let n = line_num(&line.id);
            if n > self.emitted {
                out.push(line.clone());
                if out.len() >= max {
                    break;
                }
            }
        }
        if let Some(last) = out.last() {
            self.emitted = line_num(&last.id);
        }
        out
    }

    /// Lines with an id greater than `after`, capped to the newest `max`.
    /// The returned cursor is the newest id seen, including lines skipped for backpressure.
    pub fn scan_since(&self, after: u64, max: usize) -> (u64, Vec<LogLine>) {
        let mut matched = Vec::new();
        let mut high = after;
        for line in &self.lines {
            let n = line_num(&line.id);
            if n > after {
                high = n;
                matched.push(line.clone());
            }
        }
        if matched.len() > max {
            let drop_n = matched.len() - max;
            matched.drain(0..drop_n);
        }
        (high, matched)
    }

    pub fn recent(&self, n: usize) -> Vec<LogLine> {
        let skip = self.lines.len().saturating_sub(n);
        self.lines.iter().skip(skip).cloned().collect()
    }

    pub fn facets(&self) -> (std::collections::HashMap<String, u64>, std::collections::HashMap<String, u64>) {
        let mut level = std::collections::HashMap::new();
        let mut app_id = std::collections::HashMap::new();
        for line in &self.lines {
            let lvl = serde_json::to_string(&line.level)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
            *level.entry(lvl).or_insert(0) += 1;
            if let Some(app) = &line.app_id {
                *app_id.entry(app.clone()).or_insert(0) += 1;
            }
        }
        (level, app_id)
    }
}

fn line_num(id: &str) -> u64 {
    id.trim_start_matches("log-").parse().unwrap_or(0)
}

fn is_continuation(raw: &str) -> bool {
    raw.starts_with(' ')
        || raw.starts_with('\t')
        || raw.starts_with("at ")
        || raw.starts_with("Caused by")
        || raw.starts_with("...")
}

fn starts_group(raw: &str) -> bool {
    raw.contains("Traceback (most recent call last)")
        || raw.contains("Exception")
        || raw.contains("Error:")
}

fn extract_http_status(raw: &str) -> Option<u16> {
    let parts: Vec<&str> = raw.split_whitespace().collect();
    for (i, p) in parts.iter().enumerate() {
        if p.contains("HTTP/1.") && i + 1 < parts.len() {
            return parts[i + 1].parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_stack_and_drains_incrementally() {
        let mut buf = LogRingBuffer::new();
        buf.push("stderr", Some("api"), "Error: boom");
        buf.push("stderr", Some("api"), "    at main (app.js:1:1)");
        let first = buf.drain_new(10);
        assert_eq!(first.len(), 2);
        assert_eq!(first[1].multiline_group_id, first[0].multiline_group_id);
        assert!(buf.drain_new(10).is_empty());
        buf.push("stderr", Some("api"), "INFO next");
        assert_eq!(buf.drain_new(10).len(), 1);
        assert_eq!(buf.recent(10).len(), 3);
    }
}

fn chrono_now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

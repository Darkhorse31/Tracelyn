use crate::apps::TracelynApp;
use crate::diagnostics::{DiagnosticEntry, DiagnosticStatus};
use crate::insight::detect::{detection_script, parse_detect_output, validate_log_path};
use crate::insight::model::OtelStats;
use crate::insight::otel::OtelAssembler;
use crate::insight::slowlog::{parse_mongo_line, parse_postgres_line, push_mysql, push_text_lines, SlowDraft};
use crate::insight::stack::{drafts_from_log_lines, LogSnippet};
use crate::insight::store;
use crate::insight::{otel::ParsedSpan, stack::ErrorDraft};
use crate::logs::level::LogLevel;
use crate::logs::LogRingBuffer;
use crate::ssh::{run_command, run_command_bytes, SshSession};
use crate::state::AppState;
use crate::storage::RawSample;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const MAX_BYTES: u64 = 262_144;
const FILES_PER_POLL: usize = 3;

pub struct InsightPoller {
    cursors: HashMap<String, FileCursor>,
    next_index: usize,
    last_poll: Option<Instant>,
    malformed: u64,
    spans_ingested: u64,
    files_ok: u32,
    files_error: u32,
    last_error: Option<String>,
    suggestions: usize,
    slow_summary: String,
    slow_reading: bool,
    slow_problem: bool,
    log_cursor: u64,
}

struct FileCursor {
    offset: u64,
    resolved: String,
    kind: String,
    app_id: String,
    otel: OtelAssembler,
    mysql: String,
    text: String,
}

impl Default for InsightPoller {
    fn default() -> Self {
        Self {
            cursors: HashMap::new(),
            next_index: 0,
            last_poll: None,
            malformed: 0,
            spans_ingested: 0,
            files_ok: 0,
            files_error: 0,
            last_error: None,
            suggestions: 0,
            slow_summary: "Slow query logs have not been checked yet. Connect the server, then open Slow queries.".into(),
            slow_reading: false,
            slow_problem: false,
            log_cursor: 0,
        }
    }
}

impl InsightPoller {
    pub fn due(&self) -> bool {
        self.last_poll
            .map(|started| started.elapsed() >= Duration::from_secs(5))
            .unwrap_or(true)
    }

    pub fn diagnostics(&self) -> Vec<DiagnosticEntry> {
        let otel_status = if self.files_error > 0 {
            DiagnosticStatus::Error
        } else if self.files_ok == 0 || self.malformed > 0 {
            DiagnosticStatus::Warning
        } else {
            DiagnosticStatus::Ok
        };
        let otel_message = if self.files_ok == 0 {
            format!(
                "No OpenTelemetry files are being read. {} suggested file(s) can be confirmed on the Traces page.",
                self.suggestions
            )
        } else {
            format!(
                "{} OpenTelemetry file(s) reading, {} span(s) ingested, {} malformed line(s).",
                self.files_ok, self.spans_ingested, self.malformed
            )
        };
        let slow_status = if self.slow_problem {
            DiagnosticStatus::Error
        } else if self.slow_reading {
            DiagnosticStatus::Ok
        } else {
            DiagnosticStatus::Warning
        };
        vec![
            DiagnosticEntry {
                collector: "otel".into(),
                status: otel_status,
                message: otel_message,
                fix_hint: self.last_error.clone().or_else(|| {
                    Some(
                        "Confirm a suggested file on the Traces page, or add the Collector file exporter path. The SSH user must be able to read it. Gzip rotations are ignored."
                            .into(),
                    )
                }),
            },
            DiagnosticEntry {
                collector: "slowlog".into(),
                status: slow_status,
                message: self.slow_summary.clone(),
                fix_hint: Some(
                    "Open Slow queries. Each database card has the exact setting to copy. Tracelyn only reads the log."
                        .into(),
                ),
            },
        ]
    }

    pub async fn detect(&mut self, session: &SshSession, app: &AppHandle, server_id: &str) {
        let dirs: Vec<(String, String)> = app
            .state::<AppState>()
            .apps_for_server(server_id)
            .iter()
            .filter_map(|row| row.cwd.clone().map(|cwd| (row.id.clone(), cwd)))
            .collect();
        let script = detection_script(&dirs);
        match run_command(session, &script).await {
            Ok(output) => {
                let found = parse_detect_output(&output);
                self.suggestions = found.suggestions.len();
                self.slow_summary = summarize_engines(&found.engines);
                self.slow_reading = found.engines.iter().any(|engine| engine.logging_on && engine.path.is_some());
                self.slow_problem = false;
                let state = app.state::<AppState>();
                let guard = state.metrics.lock();
                let saved = store::save_detection(guard.connection(), server_id, &found);
                drop(guard);
                if let Err(err) = saved {
                    self.last_error = Some(err.to_string());
                }
            }
            Err(err) => {
                self.slow_problem = true;
                self.slow_summary = format!("Could not read database config files: {err}");
                self.last_error = Some(err.to_string());
            }
        }
    }

    pub async fn poll(&mut self, session: &SshSession, app: &AppHandle, server_id: &str, logs: &mut LogRingBuffer) {
        self.last_poll = Some(Instant::now());
        self.scan_app_logs(app, server_id, logs);
        let watched = {
            let state = app.state::<AppState>();
            let guard = state.metrics.lock();
            store::watched_files(guard.connection(), server_id)
        };
        let files = match watched {
            Ok(files) => files,
            Err(err) => {
                self.last_error = Some(err.to_string());
                return;
            }
        };
        if files.is_empty() {
            self.persist_stats(app, server_id);
            return;
        }
        let count = files.len();
        let start = self.next_index % count;
        self.next_index = self.next_index.wrapping_add(FILES_PER_POLL);
        let batch: Vec<_> = (0..FILES_PER_POLL.min(count))
            .map(|step| files[(start + step) % count].clone())
            .collect();
        if batch.iter().any(|file| file.kind == "otel") {
            self.files_ok = 0;
            self.files_error = 0;
        }
        for file in &batch {
            self.read_file(session, app, server_id, logs, file).await;
        }
        self.persist_stats(app, server_id);
    }

    async fn read_file(
        &mut self,
        session: &SshSession,
        app: &AppHandle,
        server_id: &str,
        logs: &mut LogRingBuffer,
        file: &crate::insight::model::WatchedFile,
    ) {
        let (offset, previous) = self
            .cursors
            .get(&file.path)
            .map(|cursor| (cursor.offset, cursor.resolved.clone()))
            .unwrap_or((0, String::new()));
        let script = match read_script(&file.path, offset, file.is_dir, &previous) {
            Ok(script) => script,
            Err(err) => {
                self.note_file_error(file, &err);
                return;
            }
        };
        let raw = match run_command_bytes(session, &script).await {
            Ok(raw) => raw,
            Err(err) => {
                self.note_file_error(file, &err.to_string());
                return;
            }
        };
        let parsed = parse_read_response(&raw);
        let outcome = {
            let cursor = self.cursors.entry(file.path.clone()).or_insert_with(|| FileCursor {
                offset: 0,
                resolved: String::new(),
                kind: file.kind.clone(),
                app_id: file.app_id.clone(),
                otel: OtelAssembler::default(),
                mysql: String::new(),
                text: String::new(),
            });
            cursor.kind = file.kind.clone();
            cursor.app_id = file.app_id.clone();
            if !parsed.file.is_empty() {
                cursor.resolved = parsed.file.clone();
            }
            match parsed.kind {
                ReadKind::Failed(message) => Err(message),
                ReadKind::Ready { size, mode, data } => Ok(apply_read(cursor, size, mode, data)),
            }
        };
        match outcome {
            Err(message) => self.note_file_error(file, &message),
            Ok(Applied::Noted) => {
                if file.kind == "otel" {
                    self.files_ok += 1;
                } else {
                    self.slow_reading = true;
                }
            }
            Ok(Applied::Fed(fed)) => {
                if file.kind == "otel" {
                    self.files_ok += 1;
                } else {
                    self.slow_reading = true;
                }
                self.finish_feed(app, server_id, logs, file, fed);
            }
        }
    }

    fn finish_feed(&mut self, app: &AppHandle, server_id: &str, logs: &mut LogRingBuffer, file: &crate::insight::model::WatchedFile, mut fed: Feed) {
        self.malformed += fed.malformed;
        self.spans_ingested += fed.spans.len() as u64;
        let apps = app.state::<AppState>().apps_for_server(server_id);
        assign_apps(&apps, file, &mut fed);
        for log in &fed.logs {
            let trace = log.trace_id.clone().unwrap_or_default();
            let span = log.span_id.clone().unwrap_or_default();
            let line = format!("{} [trace={trace} span={span}] {}", log.severity, log.body);
            logs.push("otel", log.app_id.as_deref(), &line);
        }
        let samples = {
            let state = app.state::<AppState>();
            let guard = state.metrics.lock();
            let saved = store::ingest(
                guard.connection(),
                server_id,
                &fed.spans,
                &fed.logs,
                &fed.metrics,
                &fed.errors,
                &fed.slow,
            );
            drop(guard);
            match saved {
                Ok(samples) => samples,
                Err(err) => {
                    self.last_error = Some(err.to_string());
                    Vec::new()
                }
            }
        };
        if !samples.is_empty() {
            let batch = samples
                .into_iter()
                .map(|(key, value, ts_ms)| RawSample {
                    server_id: server_id.to_string(),
                    metric_key: key,
                    ts_ms,
                    value,
                })
                .collect::<Vec<_>>();
            let state = app.state::<AppState>();
            let _ = state.metrics.lock().insert_raw_batch(&batch);
        }
    }

    fn scan_app_logs(&mut self, app: &AppHandle, server_id: &str, logs: &LogRingBuffer) {
        let (next, lines) = logs.scan_since(self.log_cursor, 400);
        self.log_cursor = next;
        let snippets = lines
            .into_iter()
            .filter(|line| line.source_id != "otel")
            .map(|line| LogSnippet {
                app_id: line.app_id,
                level: level_name(line.level).into(),
                raw: line.raw,
                ts_ms: line.ts_ms as i64,
            })
            .collect::<Vec<_>>();
        let drafts = drafts_from_log_lines(&snippets);
        if drafts.is_empty() {
            return;
        }
        let state = app.state::<AppState>();
        let guard = state.metrics.lock();
        let saved = store::ingest(guard.connection(), server_id, &[], &[], &[], &drafts, &[]);
        drop(guard);
        if let Err(err) = saved {
            self.last_error = Some(err.to_string());
        }
    }

    fn note_file_error(&mut self, file: &crate::insight::model::WatchedFile, message: &str) {
        self.last_error = Some(format!("{}: {message}", file.path));
        if file.kind == "otel" {
            self.files_error += 1;
        }
        if file.kind != "otel" {
            self.slow_problem = true;
            self.slow_summary = format!(
                "Cannot read the {} log at {}: {message}. Grant the SSH user read access, for example by adding them to the adm group.",
                file.kind, file.path
            );
        }
    }

    fn persist_stats(&self, app: &AppHandle, server_id: &str) {
        let stats = OtelStats {
            malformed: self.malformed,
            spans_ingested: self.spans_ingested,
            files_ok: self.files_ok,
            files_error: self.files_error,
            last_error: self.last_error.clone(),
        };
        let state = app.state::<AppState>();
        let guard = state.metrics.lock();
        let saved = store::save_stats(guard.connection(), server_id, &stats);
        drop(guard);
        let _ = saved;
    }
}

struct Feed {
    spans: Vec<ParsedSpan>,
    logs: Vec<crate::insight::otel::ParsedLog>,
    metrics: Vec<crate::insight::otel::ParsedMetric>,
    errors: Vec<ErrorDraft>,
    slow: Vec<SlowDraft>,
    malformed: u64,
    bytes: u64,
}

fn feed_text(cursor: &mut FileCursor, text: &str) -> Feed {
    let bytes = text.len() as u64;
    match cursor.kind.as_str() {
        "mysql" => {
            let (slow, malformed) = push_mysql(&mut cursor.mysql, text);
            Feed::slow(slow, malformed, bytes)
        }
        "postgres" => {
            let (lines, mut malformed) = push_text_lines(&mut cursor.text, text);
            let mut slow = Vec::new();
            for line in lines {
                if line.to_ascii_lowercase().contains("duration:") {
                    match parse_postgres_line(&line) {
                        Some(row) => slow.push(row),
                        None => malformed += 1,
                    }
                }
            }
            Feed::slow(slow, malformed, bytes)
        }
        "mongo" => {
            let (lines, mut malformed) = push_text_lines(&mut cursor.text, text);
            let mut slow = Vec::new();
            for line in lines {
                let lower = line.to_ascii_lowercase();
                if lower.contains("slow query") {
                    match parse_mongo_line(&line) {
                        Some(row) => slow.push(row),
                        None => malformed += 1,
                    }
                }
            }
            Feed::slow(slow, malformed, bytes)
        }
        _ => {
            let before = cursor.otel.malformed;
            let batch = cursor.otel.push(text);
            Feed {
                malformed: cursor.otel.malformed.saturating_sub(before),
                spans: batch.spans,
                logs: batch.logs,
                metrics: batch.metrics,
                errors: batch.errors,
                slow: Vec::new(),
                bytes,
            }
        }
    }
}

impl Feed {
    fn slow(slow: Vec<SlowDraft>, malformed: u64, bytes: u64) -> Self {
        Self {
            spans: Vec::new(),
            logs: Vec::new(),
            metrics: Vec::new(),
            errors: Vec::new(),
            slow,
            malformed,
            bytes,
        }
    }
}

fn assign_apps(apps: &[TracelynApp], file: &crate::insight::model::WatchedFile, fed: &mut Feed) {
    let file_app = if file.app_id.is_empty() { None } else { Some(file.app_id.clone()) };
    for span in &mut fed.spans {
        span.app_id = file_app.clone().or_else(|| match_service(apps, &span.service));
    }
    for log in &mut fed.logs {
        log.app_id = file_app.clone().or_else(|| match_service(apps, &log.service));
    }
    for error in &mut fed.errors {
        if error.app_id.is_none() {
            error.app_id = file_app.clone();
        }
    }
    let db_app = match_db(apps, &file.kind);
    for query in &mut fed.slow {
        if query.app_id.is_none() {
            query.app_id = db_app.clone();
        }
    }
}

fn match_service(apps: &[TracelynApp], service: &str) -> Option<String> {
    apps.iter()
        .find(|app| app.name.eq_ignore_ascii_case(service))
        .map(|app| app.id.clone())
}

fn match_db(apps: &[TracelynApp], engine: &str) -> Option<String> {
    let needles: &[&str] = match engine {
        "mysql" => &["mysql", "mariadb"],
        "postgres" => &["postgres", "postgresql"],
        "mongo" => &["mongo"],
        _ => return None,
    };
    let hits: Vec<_> = apps
        .iter()
        .filter(|app| {
            let name = app.name.to_ascii_lowercase();
            needles.iter().any(|needle| name.contains(needle))
        })
        .collect();
    if hits.len() == 1 {
        Some(hits[0].id.clone())
    } else {
        None
    }
}

fn level_name(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "error",
        LogLevel::Fatal => "fatal",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
        LogLevel::Trace => "trace",
        LogLevel::Unknown => "unknown",
    }
}

fn summarize_engines(engines: &[crate::insight::model::SlowEngineStatus]) -> String {
    let reading: Vec<_> = engines
        .iter()
        .filter(|engine| engine.logging_on && engine.path.is_some())
        .map(|engine| engine.engine.as_str())
        .collect();
    if reading.is_empty() {
        "No MySQL, PostgreSQL, or MongoDB slow log is enabled. The Slow queries page lists the setting for each.".into()
    } else {
        format!("Slow log reading for {}.", reading.join(", "))
    }
}

enum ReadKind {
    Failed(String),
    Ready { size: u64, mode: ReadMode, data: Vec<u8> },
}

enum ReadMode {
    Ok,
    Trunc,
    Same,
}

struct ParsedRead {
    file: String,
    kind: ReadKind,
}

enum Applied {
    Noted,
    Fed(Feed),
}

fn apply_read(cursor: &mut FileCursor, size: u64, mode: ReadMode, data: Vec<u8>) -> Applied {
    match mode {
        ReadMode::Same => Applied::Noted,
        ReadMode::Trunc => {
            cursor.offset = size;
            cursor.otel.partial.clear();
            cursor.mysql.clear();
            cursor.text.clear();
            Applied::Noted
        }
        ReadMode::Ok => {
            if data.starts_with(&[0x1f, 0x8b]) {
                cursor.offset = cursor.offset.saturating_add(data.len() as u64);
                return Applied::Fed(Feed::slow(Vec::new(), 1, 0));
            }
            match String::from_utf8(data) {
                Ok(text) => {
                    let fed = feed_text(cursor, &text);
                    cursor.offset = cursor.offset.saturating_add(fed.bytes);
                    Applied::Fed(fed)
                }
                Err(err) => {
                    cursor.offset = cursor.offset.saturating_add(err.as_bytes().len() as u64);
                    Applied::Fed(Feed::slow(Vec::new(), 1, 0))
                }
            }
        }
    }
}

fn read_script(path: &str, offset: u64, is_dir: bool, previous: &str) -> Result<String, String> {
    validate_log_path(path)?;
    if !previous.is_empty() {
        validate_log_path(previous)?;
    }
    let start = offset.saturating_add(1);
    let previous = if previous.is_empty() { path } else { previous };
    if is_dir {
        return Ok(format!(
            "f=$(ls -1t '{path}'/*.log '{path}'/*.json '{path}'/*.jsonl 2>/dev/null | head -n 1)\n\
             off={offset}\n\
             if [ \"$f\" != '{previous}' ]; then off=0; fi\n\
             if [ -z \"$f\" ] || [ ! -f \"$f\" ]; then echo FILE; echo 'SIZE MISSING'; echo 'MODE ERR'; exit 0; fi\n\
             echo \"FILE $f\"\n\
             sz=$(stat -c %s -- \"$f\" 2>&1) || true\n\
             case \"$sz\" in\n\
               ''|*[!0-9]*) printf 'SIZE ERR %s\\nMODE ERR\\n' \"$sz\" ;;\n\
               *) if [ \"$sz\" -lt \"$off\" ]; then printf 'SIZE %s\\nMODE TRUNC\\n' \"$sz\";\n\
                  elif [ \"$sz\" -eq \"$off\" ]; then printf 'SIZE %s\\nMODE SAME\\n' \"$sz\";\n\
                  else printf 'SIZE %s\\nMODE OK\\n' \"$sz\"; start=$((off + 1)); timeout 2 tail -c +\"$start\" -- \"$f\" | head -c {MAX_BYTES}; fi ;;\n\
             esac\n"
        ));
    }
    Ok(format!(
        "f='{path}'\n\
         if [ ! -f \"$f\" ]; then echo FILE; echo 'SIZE MISSING'; echo 'MODE ERR'; exit 0; fi\n\
         echo \"FILE $f\"\n\
         sz=$(stat -c %s -- \"$f\" 2>&1) || true\n\
         case \"$sz\" in\n\
           ''|*[!0-9]*) printf 'SIZE ERR %s\\nMODE ERR\\n' \"$sz\" ;;\n\
           *) if [ \"$sz\" -lt {offset} ]; then printf 'SIZE %s\\nMODE TRUNC\\n' \"$sz\";\n\
              elif [ \"$sz\" -eq {offset} ]; then printf 'SIZE %s\\nMODE SAME\\n' \"$sz\";\n\
              else printf 'SIZE %s\\nMODE OK\\n' \"$sz\"; timeout 2 tail -c +{start} -- \"$f\" | head -c {MAX_BYTES}; fi ;;\n\
         esac\n"
    ))
}

fn parse_read_response(raw: &[u8]) -> ParsedRead {
    let mut parts = raw.splitn(3, |byte| *byte == b'\n');
    let file_line = String::from_utf8_lossy(parts.next().unwrap_or(&[])).trim().to_string();
    let size_line = String::from_utf8_lossy(parts.next().unwrap_or(&[])).trim().to_string();
    let rest = parts.next().unwrap_or(&[]);
    let file = file_line.strip_prefix("FILE").unwrap_or("").trim().to_string();
    let (mode_line, data) = match rest.iter().position(|byte| *byte == b'\n') {
        Some(index) => (
            String::from_utf8_lossy(&rest[..index]).trim().to_string(),
            rest[index + 1..].to_vec(),
        ),
        None => (String::from_utf8_lossy(rest).trim().to_string(), Vec::new()),
    };
    let kind = if size_line == "SIZE MISSING" {
        ReadKind::Failed("file is missing".into())
    } else if let Some(message) = size_line.strip_prefix("SIZE ERR") {
        ReadKind::Failed(message.trim().to_string())
    } else if let Some(number) = size_line.strip_prefix("SIZE ") {
        match number.trim().parse::<u64>() {
            Ok(size) => {
                let mode = if mode_line.contains("TRUNC") {
                    ReadMode::Trunc
                } else if mode_line.contains("SAME") {
                    ReadMode::Same
                } else if mode_line.contains("OK") {
                    ReadMode::Ok
                } else {
                    return ParsedRead {
                        file,
                        kind: ReadKind::Failed(format!("unexpected read mode {mode_line}")),
                    };
                };
                ReadKind::Ready { size, mode, data }
            }
            Err(_) => ReadKind::Failed(size_line),
        }
    } else {
        ReadKind::Failed(if size_line.is_empty() { "empty response".into() } else { size_line })
    };
    ParsedRead { file, kind }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_size_protocol_and_feeds_otel() {
        let raw = b"FILE /var/log/otel/traces.json\nSIZE 42\nMODE OK\n{\"traceId\":\"abc\",\"spanId\":\"def\",\"name\":\"work\",\"kind\":3,\"startTimeUnixNano\":\"1000\",\"endTimeUnixNano\":\"5000\",\"attributes\":[{\"key\":\"peer.service\",\"value\":{\"stringValue\":\"auth\"}}]}\n";
        let parsed = parse_read_response(raw);
        assert_eq!(parsed.file, "/var/log/otel/traces.json");
        let ReadKind::Ready { mode, data, .. } = parsed.kind else {
            panic!("expected data");
        };
        assert!(matches!(mode, ReadMode::Ok));
        let mut cursor = FileCursor {
            offset: 0,
            resolved: String::new(),
            kind: "otel".into(),
            app_id: "app".into(),
            otel: OtelAssembler::default(),
            mysql: String::new(),
            text: String::new(),
        };
        let text = String::from_utf8(data).unwrap();
        let fed = feed_text(&mut cursor, &text);
        assert_eq!(fed.spans.len(), 1);
        assert_eq!(fed.spans[0].peer.as_deref(), Some("auth"));
        assert_eq!(fed.malformed, 0);

        let trunc = parse_read_response(b"FILE /tmp/a.json\nSIZE 0\nMODE TRUNC\n");
        assert!(matches!(trunc.kind, ReadKind::Ready { mode: ReadMode::Trunc, size: 0, .. }));
        let missing = parse_read_response(b"FILE\nSIZE MISSING\nMODE ERR\n");
        assert!(matches!(missing.kind, ReadKind::Failed(_)));
    }
}

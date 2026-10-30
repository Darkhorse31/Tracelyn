use crate::apps::TracelynApp;
use crate::diagnostics::{DiagnosticEntry, DiagnosticStatus};
use crate::insight::detect::{detection_script, parse_detect_output, validate_log_path};
use crate::insight::model::{OtelReport, OtelStats};
use crate::insight::otel::OtelAssembler;
use crate::insight::slowlog::{parse_mongo_line, parse_postgres_line, push_mysql, push_text_lines, SlowDraft};
use crate::insight::stack::{drafts_from_log_lines, LogSnippet};
use crate::insight::store;
use crate::insight::{otel::ParsedSpan, stack::ErrorDraft};
use crate::logs::level::LogLevel;
use crate::logs::LogRingBuffer;
use crate::ssh::{run_command_bytes_stdin, SshSession};
use crate::state::AppState;
use crate::storage::RawSample;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const MAX_BYTES: u64 = 1_048_576;
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
    otel_reason: String,
    otel_detail: String,
    otel_sample: Option<String>,
    otel_collector: String,
    otel_last_write: Option<String>,
    flag_spans: bool,
    flag_parse: bool,
    flag_empty: bool,
    flag_deny: bool,
    flag_missing: bool,
    saw_otel: bool,
}

struct FileCursor {
    offset: u64,
    resolved: String,
    kind: String,
    app_id: String,
    otel: OtelAssembler,
    mysql: String,
    text: String,
    /// mtime of the last read. A same-size rewrite (file exporter append off) changes mtime without growing the file.
    stamp: u64,
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
            otel_reason: "no_config".into(),
            otel_detail: "No collector config was found yet. Connect the server so Tracelyn can read the file exporter.".into(),
            otel_sample: None,
            otel_collector: "unknown".into(),
            otel_last_write: None,
            flag_spans: false,
            flag_parse: false,
            flag_empty: false,
            flag_deny: false,
            flag_missing: false,
            saw_otel: false,
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
        let otel_message = if !self.otel_detail.is_empty() && self.files_ok == 0 {
            self.otel_detail.clone()
        } else if self.files_ok == 0 {
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
                        "Use Set up monitoring so Tracelyn can read the collector file. Gzip rotations are ignored."
                            .into(),
                    )
                }),
            },
            DiagnosticEntry {
                collector: "slowlog".into(),
                status: slow_status,
                message: self.slow_summary.clone(),
                fix_hint: Some(
                    "Open Slow queries. Check which databases are ready for monitoring."
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
        let secret = app.state::<crate::state::AppState>().sudo_password(server_id);
        let wrapped = if secret.is_some() {
            crate::setup::sudo_stdin_read(&script)
        } else {
            crate::setup::privileged_read(&script)
        };
        let stdin = secret.as_deref().map(crate::setup::password_bytes);
        match crate::ssh::run_command_captured(session, &wrapped, stdin).await.map(|out| out.stdout) {
            Ok(output) => {
                let found = parse_detect_output(&output);
                self.suggestions = found.suggestions.len();
                self.slow_summary = summarize_engines(&found.engines);
                self.slow_reading = found.engines.iter().any(|engine| engine.logging_on && engine.path.is_some());
                self.slow_problem = false;
                let state = app.state::<AppState>();
                let guard = state.metrics.lock();
                let saved = store::save_detection(guard.connection(), server_id, &found)
                    .and_then(|_| store::adopt_otel_paths(guard.connection(), server_id, &found.otel.watch))
                    .and_then(|_| store::merge_otel_report(guard.connection(), server_id, &found.otel));
                drop(guard);
                if let Err(err) = saved {
                    self.last_error = Some(err.to_string());
                }
                self.apply_report(&found.otel);
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
        self.flag_spans = false;
        self.flag_parse = false;
        self.flag_empty = false;
        self.flag_deny = false;
        self.flag_missing = false;
        self.saw_otel = false;
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
        self.finish_otel_flags();
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
        let known = self
            .cursors
            .get(&file.path)
            .map(|cursor| (cursor.offset, cursor.resolved.clone()));
        let (offset, previous) = if let Some(pair) = known {
            pair
        } else if file.kind == "otel" {
            let state = app.state::<AppState>();
            let guard = state.metrics.lock();
            let stored = store::file_offset(guard.connection(), server_id, &file.path).unwrap_or(0);
            (stored, String::new())
        } else {
            (0, String::new())
        };
        let (stamp, cont) = self
            .cursors
            .get(&file.path)
            .map(|cursor| (cursor.stamp, !cursor.otel.partial.is_empty()))
            .unwrap_or((0, false));
        let script = match read_script(
            &file.path,
            offset,
            file.is_dir,
            &previous,
            file.kind == "otel",
            stamp,
            cont,
        ) {
            Ok(script) => script,
            Err(err) => {
                self.note_file_error(file, &err);
                return;
            }
        };
        let secret = app.state::<crate::state::AppState>().sudo_password(server_id);
        let wrapped = if secret.is_some() {
            crate::setup::sudo_stdin_read(&script)
        } else {
            crate::setup::privileged_read(&script)
        };
        let stdin = secret.as_deref().map(crate::setup::password_bytes);
        let raw = match run_command_bytes_stdin(session, &wrapped, stdin).await {
            Ok(raw) => raw,
            Err(err) => {
                self.note_file_error(file, &err.to_string());
                return;
            }
        };
        let parsed = parse_read_response(&raw);
        let outcome = {
            let cursor = self.cursors.entry(file.path.clone()).or_insert_with(|| FileCursor {
                offset,
                resolved: String::new(),
                kind: file.kind.clone(),
                app_id: file.app_id.clone(),
                otel: OtelAssembler::default(),
                mysql: String::new(),
                text: String::new(),
                stamp: 0,
            });
            cursor.kind = file.kind.clone();
            cursor.app_id = file.app_id.clone();
            if !parsed.file.is_empty() {
                cursor.resolved = parsed.file.clone();
            }
            let stamp = parsed.stamp;
            let start = parsed.start;
            match parsed.kind {
                ReadKind::Failed(message) => Err(message),
                ReadKind::Ready { size: _, mode, data } => Ok(apply_read(cursor, mode, data, stamp, start)),
            }
        };
        match outcome {
            Err(message) => self.note_file_error(file, &message),
            Ok(Applied::Noted) => {
                if file.kind == "otel" {
                    self.files_ok += 1;
                    self.saw_otel = true;
                    if self.cursors.get(&file.path).map(|cursor| cursor.offset).unwrap_or(1) == 0 {
                        self.flag_empty = true;
                    }
                } else {
                    self.slow_reading = true;
                }
            }
            Ok(Applied::Truncated) => {
                if file.kind == "otel" {
                    self.files_ok += 1;
                    self.saw_otel = true;
                } else {
                    self.slow_reading = true;
                }
            }
            Ok(Applied::Fed(fed)) => {
                if file.kind == "otel" {
                    self.files_ok += 1;
                    self.saw_otel = true;
                    if !fed.spans.is_empty() {
                        self.flag_spans = true;
                    } else if fed.malformed > 0 {
                        self.flag_parse = true;
                        if self.otel_sample.is_none() && !fed.sample.is_empty() {
                            self.otel_sample = Some(fed.sample.clone());
                        }
                    } else if fed.bytes == 0 {
                        self.flag_empty = true;
                    }
                } else {
                    self.slow_reading = true;
                }
                self.finish_feed(app, server_id, logs, file, fed);
            }
        }
        if file.kind == "otel" {
            if let Some(cursor) = self.cursors.get(&file.path) {
                let persist = cursor.offset.saturating_sub(cursor.otel.partial.len() as u64);
                let state = app.state::<AppState>();
                let guard = state.metrics.lock();
                let _ = store::save_file_offset(guard.connection(), server_id, &file.path, persist);
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
            self.saw_otel = true;
            let lower = message.to_ascii_lowercase();
            if lower.contains("permission") || lower.contains("denied") {
                self.flag_deny = true;
                self.last_error = Some(
                    "The trace file exists but this login cannot read it. Use Set up monitoring to allow access.".into(),
                );
            } else if lower.contains("missing") {
                self.flag_missing = true;
                self.last_error = Some("The trace file is not on the server yet.".into());
            } else {
                self.flag_deny = true;
            }
        }
        if file.kind != "otel" {
            self.slow_problem = true;
            self.slow_summary = format!(
                "Cannot read the {} log at {}: {message}. Open monitoring setup to check access.",
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
            reason: self.otel_reason.clone(),
            detail: self.otel_detail.clone(),
            sample: self.otel_sample.clone(),
            collector: self.otel_collector.clone(),
            last_write: self.otel_last_write.clone(),
        };
        let state = app.state::<AppState>();
        let guard = state.metrics.lock();
        let saved = store::save_stats(guard.connection(), server_id, &stats);
        drop(guard);
        let _ = saved;
    }

    fn apply_report(&mut self, report: &OtelReport) {
        self.otel_reason = report.reason.clone();
        self.otel_detail = report.detail.clone();
        self.otel_sample = report.sample.clone();
        self.otel_collector = report.collector.clone();
        self.otel_last_write = report.last_write.clone();
    }

    fn finish_otel_flags(&mut self) {
        if !self.saw_otel {
            return;
        }
        if self.flag_spans || self.spans_ingested > 0 {
            self.otel_reason = "ok".into();
            self.otel_detail = format!(
                "Reading the collector file. {} span(s) stored.",
                self.spans_ingested
            );
            return;
        }
        if self.flag_parse {
            self.otel_reason = "parse_error".into();
            self.otel_detail = "A line in the collector file is not OTLP JSON.".into();
            return;
        }
        if self.flag_deny {
            self.otel_reason = "unreadable".into();
            self.otel_detail = self
                .last_error
                .clone()
                .unwrap_or_else(|| "The collector file is not readable by the SSH user.".into());
            return;
        }
        if self.flag_missing {
            self.otel_reason = "missing".into();
            self.otel_detail = self
                .last_error
                .clone()
                .unwrap_or_else(|| "The collector file is not on disk.".into());
            return;
        }
        if self.flag_empty {
            self.otel_reason = "empty".into();
            self.otel_detail = "The collector file is empty.".into();
        }
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
    sample: String,
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
            let sample = text
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.chars().take(180).collect())
                .unwrap_or_default();
            Feed {
                malformed: cursor.otel.malformed.saturating_sub(before),
                spans: batch.spans,
                logs: batch.logs,
                metrics: batch.metrics,
                errors: batch.errors,
                slow: Vec::new(),
                bytes,
                sample,
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
            sample: String::new(),
        }
    }
}

fn assign_apps(apps: &[TracelynApp], file: &crate::insight::model::WatchedFile, fed: &mut Feed) {
    let file_app = if file.app_id.is_empty() { None } else { Some(file.app_id.clone()) };
    for span in &mut fed.spans {
        span.app_id = match_service(apps, &span.service).or_else(|| file_app.clone());
    }
    for log in &mut fed.logs {
        log.app_id = match_service(apps, &log.service).or_else(|| file_app.clone());
    }
    for error in &mut fed.errors {
        if error.app_id.is_none() {
            error.app_id = error
                .trace_id
                .as_ref()
                .and_then(|trace| fed.spans.iter().find(|span| span.trace_id == *trace))
                .and_then(|span| span.app_id.clone())
                .or_else(|| file_app.clone());
        }
    }
    let db_app = match_db(apps, &file.kind);
    for query in &mut fed.slow {
        if query.app_id.is_none() {
            query.app_id = db_app.clone();
        }
    }
}

fn cwd_name(cwd: Option<&str>) -> &str {
    cwd.and_then(|path| path.rsplit(['/', '\\']).find(|part| !part.is_empty()))
        .unwrap_or("")
}

fn match_service(apps: &[TracelynApp], service: &str) -> Option<String> {
    let service = service.trim();
    if service.is_empty() || service.eq_ignore_ascii_case("unknown") {
        return None;
    }
    if let Some(app) = apps.iter().find(|app| {
        app.name.eq_ignore_ascii_case(service) || cwd_name(app.cwd.as_deref()).eq_ignore_ascii_case(service)
    }) {
        return Some(app.id.clone());
    }
    let hits: Vec<_> = apps
        .iter()
        .filter(|app| {
            let name = app.name.to_ascii_lowercase();
            let svc = service.to_ascii_lowercase();
            (name.len() >= 3 && svc.contains(&name)) || (svc.len() >= 3 && name.contains(&svc))
        })
        .collect();
    if hits.len() == 1 {
        Some(hits[0].id.clone())
    } else {
        None
    }
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
    Ready {
        #[allow(dead_code)]
        size: u64,
        mode: ReadMode,
        data: Vec<u8>,
    },
}

enum ReadMode {
    Ok,
    Trunc,
    Same,
}

struct ParsedRead {
    file: String,
    kind: ReadKind,
    stamp: u64,
    /// 1-based byte where `data` begins. 0 means keep the cursor and append.
    start: u64,
}

enum Applied {
    Noted,
    Truncated,
    Fed(Feed),
}

fn apply_read(cursor: &mut FileCursor, mode: ReadMode, data: Vec<u8>, stamp: u64, start: u64) -> Applied {
    if stamp > 0 {
        cursor.stamp = stamp;
    }
    match mode {
        ReadMode::Same => Applied::Noted,
        ReadMode::Trunc => {
            cursor.offset = start.saturating_sub(1);
            cursor.otel.partial.clear();
            cursor.mysql.clear();
            cursor.text.clear();
            if data.is_empty() {
                Applied::Truncated
            } else {
                feed_bytes(cursor, data)
            }
        }
        ReadMode::Ok => {
            if start > 0 {
                cursor.offset = start.saturating_sub(1);
            }
            if data.is_empty() {
                Applied::Noted
            } else {
                feed_bytes(cursor, data)
            }
        }
    }
}

fn feed_bytes(cursor: &mut FileCursor, data: Vec<u8>) -> Applied {
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

fn read_script(
    path: &str,
    offset: u64,
    is_dir: bool,
    previous: &str,
    track_rewrite: bool,
    stamp: u64,
    cont: bool,
) -> Result<String, String> {
    validate_log_path(path)?;
    if !previous.is_empty() {
        validate_log_path(previous)?;
    }
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
                  else printf 'SIZE %s\\nMODE OK\\n' \"$sz\"; start=$((off + 1)); timeout 8 tail -c +\"$start\" -- \"$f\" | head -c {MAX_BYTES}; fi ;;\n\
             esac\n"
        ));
    }
    let cont = i32::from(cont || !track_rewrite);
    // ponytail: mtime catches a same-size rewrite. A larger rewrite that still begins with '{' is read as an append.
    Ok(format!(
        "f='{path}'\n\
         off={offset}\n\
         prev_mt={stamp}\n\
         cont={cont}\n\
         if [ ! -r \"$f\" ] && [ -e \"$f\" ]; then echo \"FILE $f\"; echo 'SIZE ERR Permission denied'; echo 'MODE ERR'; exit 0; fi\n\
         if [ ! -f \"$f\" ]; then echo FILE; echo 'SIZE MISSING'; echo 'MODE ERR'; exit 0; fi\n\
         echo \"FILE $f\"\n\
         sz=$(stat -c %s -- \"$f\" 2>&1) || true\n\
         mt=$(stat -c %Y -- \"$f\" 2>/dev/null || echo 0)\n\
         case \"$sz\" in\n\
           ''|*[!0-9]*) printf 'SIZE ERR %s\\nMODE ERR\\n' \"$sz\" ;;\n\
           *)\n\
             mode=OK\n\
             start=$((off + 1))\n\
             if [ \"$sz\" -lt \"$off\" ]; then mode=TRUNC; start=1\n\
             elif [ \"$sz\" -eq \"$off\" ]; then\n\
               if [ \"$prev_mt\" != 0 ] && [ \"$mt\" != \"$prev_mt\" ]; then mode=TRUNC; start=1\n\
               else mode=SAME; start=0\n\
               fi\n\
             elif [ \"$off\" -eq 0 ] && [ \"$sz\" -gt {max} ]; then start=$((sz - {max} + 1))\n\
             elif [ \"$cont\" = 0 ] && [ \"$off\" -gt 0 ]; then\n\
               peek=$(tail -c +\"$start\" -- \"$f\" | tr -cd '[:graph:]' | head -c 1)\n\
               case \"$peek\" in\n\
                 \"{{\"|\"[\") ;;\n\
                 \"\") mode=SAME; start=0 ;;\n\
                 *) mode=TRUNC; start=1 ;;\n\
               esac\n\
             fi\n\
             if [ \"$mode\" = TRUNC ] && [ \"$sz\" -gt {max} ]; then start=$((sz - {max} + 1)); fi\n\
             if [ \"$start\" -gt 1 ] && {{ [ \"$off\" -eq 0 ] || [ \"$mode\" = TRUNC ]; }}; then skip=$(tail -c +\"$start\" -- \"$f\" | head -n 1 | wc -c); start=$((start + skip)); fi\n\
             if [ \"$mode\" = SAME ]; then printf 'SIZE %s\\nMODE SAME %s 0\\n' \"$sz\" \"$mt\"\n\
             elif [ \"$mode\" = TRUNC ]; then printf 'SIZE %s\\nMODE TRUNC %s %s\\n' \"$sz\" \"$mt\" \"$start\"; timeout 8 tail -c +\"$start\" -- \"$f\" | head -c {max}\n\
             else printf 'SIZE %s\\nMODE OK %s %s\\n' \"$sz\" \"$mt\" \"$start\"; timeout 8 tail -c +\"$start\" -- \"$f\" | head -c {max}\n\
             fi ;;\n\
         esac\n",
        path = path,
        offset = offset,
        stamp = stamp,
        cont = cont,
        max = MAX_BYTES,
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
    let mut mode_parts = mode_line.split_whitespace();
    let _mode_tag = mode_parts.next();
    let mode_kind = mode_parts.next().unwrap_or("");
    let stamp = mode_parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    let start = mode_parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);
    let kind = if size_line == "SIZE MISSING" {
        ReadKind::Failed("file is missing".into())
    } else if let Some(message) = size_line.strip_prefix("SIZE ERR") {
        ReadKind::Failed(message.trim().to_string())
    } else if let Some(number) = size_line.strip_prefix("SIZE ") {
        match number.trim().parse::<u64>() {
            Ok(size) => {
                let mode = if mode_kind == "TRUNC" {
                    ReadMode::Trunc
                } else if mode_kind == "SAME" {
                    ReadMode::Same
                } else if mode_kind == "OK" {
                    ReadMode::Ok
                } else {
                    return ParsedRead {
                        file,
                        kind: ReadKind::Failed(format!("unexpected read mode {mode_line}")),
                        stamp,
                        start,
                    };
                };
                ReadKind::Ready { size, mode, data }
            }
            Err(_) => ReadKind::Failed(size_line),
        }
    } else {
        ReadKind::Failed(if size_line.is_empty() { "empty response".into() } else { size_line })
    };
    ParsedRead { file, kind, stamp, start }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_logs_are_not_treated_as_directories() {
        let script = read_script("/var/log/mysql/slow.log", 0, false, "", false, 0, false).unwrap();
        assert!(!script.contains("ls -1t"));
        assert!(script.contains("cont=1"));
        assert!(script.contains("Permission denied"));
    }

    #[test]
    fn tail_window_aligns_records_and_bounds_reads() {
        let script = read_script("/var/log/otel/telemetry.json", 0, false, "", true, 0, false).unwrap();
        assert!(script.contains("sz - 1048576 + 1"));
        assert!(script.contains("start=$((start + skip))"));
        assert!(script.contains("head -c 1048576"));
        assert!(read_script("/tmp/x'bad", 0, false, "", true, 0, false).is_err());
    }

    #[test]
    fn permission_protocol_is_an_error_not_an_empty_file() {
        let parsed = parse_read_response(b"FILE /var/log/otel/telemetry.json\nSIZE ERR Permission denied\nMODE ERR\n");
        assert!(matches!(parsed.kind, ReadKind::Failed(reason) if reason == "Permission denied"));
    }

    #[test]
    fn parses_size_protocol_and_feeds_otel() {
        let raw = b"FILE /var/log/otel/traces.json\nSIZE 42\nMODE OK 1700000000 1\n{\"traceId\":\"abc\",\"spanId\":\"def\",\"name\":\"work\",\"kind\":3,\"startTimeUnixNano\":\"1000\",\"endTimeUnixNano\":\"5000\",\"attributes\":[{\"key\":\"peer.service\",\"value\":{\"stringValue\":\"auth\"}}]}\n";
        let parsed = parse_read_response(raw);
        assert_eq!(parsed.file, "/var/log/otel/traces.json");
        assert_eq!(parsed.stamp, 1_700_000_000);
        assert_eq!(parsed.start, 1);
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
            stamp: 0,
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

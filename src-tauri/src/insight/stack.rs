use crate::insight::fingerprint_hash;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorDraft {
    pub language: String,
    pub title: String,
    pub fingerprint: String,
    pub normalized_stack: String,
    pub message: String,
    pub trace_id: Option<String>,
    pub app_id: Option<String>,
    pub ts_ms: i64,
}

#[derive(Debug, Clone)]
pub struct LogSnippet {
    pub app_id: Option<String>,
    pub level: String,
    pub raw: String,
    pub ts_ms: i64,
}

pub fn from_exception(
    type_name: &str,
    message: &str,
    stack: &str,
    trace_id: Option<String>,
    ts_ms: i64,
) -> ErrorDraft {
    let body = if stack.trim().is_empty() {
        format!("{type_name}: {message}")
    } else {
        stack.to_string()
    };
    let mut draft = fingerprint_text(&body).unwrap_or_else(|| text_draft(&body));
    if !type_name.is_empty() {
        let title = strip_ids(&format!("{type_name}: {message}"));
        draft.title = truncate_chars(&title, 180);
    }
    draft.trace_id = trace_id.filter(|s| !s.is_empty());
    draft.ts_ms = ts_ms;
    draft
}

pub fn fingerprint_text(text: &str) -> Option<ErrorDraft> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let language = detect_language(trimmed);
    let frames = normalize_frames(language, trimmed);
    let title_raw = title_line(language, trimmed);
    let title = truncate_chars(&strip_ids(&title_raw), 180);
    let basis = if frames.is_empty() {
        strip_ids(&title_raw)
    } else {
        frames.join("\n")
    };
    if basis.trim().is_empty() {
        return None;
    }
    Some(ErrorDraft {
        fingerprint: fingerprint_hash(&format!("{language}\n{basis}")),
        language: language.to_string(),
        title,
        normalized_stack: if frames.is_empty() {
            strip_ids(&title_raw)
        } else {
            frames.join("\n")
        },
        message: truncate_chars(first_line(trimmed), 500),
        trace_id: None,
        app_id: None,
        ts_ms: 0,
    })
}

pub fn drafts_from_log_lines(lines: &[LogSnippet]) -> Vec<ErrorDraft> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !is_starter(&lines[i]) {
            i += 1;
            continue;
        }
        let mut blob = lines[i].raw.clone();
        let app = lines[i].app_id.clone();
        let ts = lines[i].ts_ms;
        let mut j = i + 1;
        while j < lines.len() && is_continuation(&lines[j].raw) && blob.len() < 16_000 {
            blob.push('\n');
            blob.push_str(&lines[j].raw);
            j += 1;
        }
        if let Some(mut draft) = fingerprint_text(&blob) {
            draft.app_id = app;
            draft.ts_ms = ts;
            out.push(draft);
        }
        i = j;
    }
    out
}

fn text_draft(text: &str) -> ErrorDraft {
    let title = truncate_chars(&strip_ids(text), 180);
    ErrorDraft {
        fingerprint: fingerprint_hash(&format!("text\n{title}")),
        language: "text".into(),
        title: title.clone(),
        normalized_stack: title,
        message: truncate_chars(first_line(text), 500),
        trace_id: None,
        app_id: None,
        ts_ms: 0,
    }
}

fn detect_language(text: &str) -> &'static str {
    if text.contains("Traceback (most recent call last)") {
        return "python";
    }
    if text.contains("goroutine ") || text.contains("panic:") {
        return "go";
    }
    if text.contains(".php(") || text.contains("PHP Fatal") || text.contains("PHP Parse") {
        return "php";
    }
    if text.contains(".cs:line") || text.contains(":line ") && text.contains(".cs") {
        return "dotnet";
    }
    if text.contains(".java:") {
        return "java";
    }
    if text.contains(".js:")
        || text.contains(".mjs:")
        || text.contains(".cjs:")
        || text.contains(".ts:")
        || text.contains("node:")
    {
        return "node";
    }
    "text"
}

fn normalize_frames(language: &str, text: &str) -> Vec<String> {
    let mut frames = Vec::new();
    for line in text.lines() {
        let frame = match language {
            "python" => norm_python_line(line),
            "node" => norm_node_line(line),
            "java" => norm_java_line(line),
            "php" => norm_php_line(line),
            "dotnet" => norm_dotnet_line(line),
            "go" => norm_go_line(line),
            _ => None,
        };
        if let Some(frame) = frame {
            let clean = strip_ids(&frame);
            if !frames.last().is_some_and(|prev| prev == &clean) {
                frames.push(clean);
            }
        }
        if frames.len() >= 40 {
            break;
        }
    }
    frames
}

fn norm_python_line(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix("File \"")?;
    let (path, after) = rest.split_once('"')?;
    let after = after.trim().strip_prefix(", line ")?;
    let (_, func) = after.split_once(", in ")?;
    Some(format!("File \"{path}\", in {}", func.trim()))
}

fn norm_node_line(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix("at ")?;
    if let Some(start) = rest.rfind('(') {
        let func = rest[..start].trim();
        let inside = rest[start + 1..].trim_end_matches(')');
        let file = strip_line_col(inside);
        if is_node_file(&file) {
            return Some(if func.is_empty() {
                format!("at {file}")
            } else {
                format!("at {func} ({file})")
            });
        }
        return None;
    }
    let file = strip_line_col(rest);
    if is_node_file(&file) {
        return Some(format!("at {file}"));
    }
    None
}

fn is_node_file(file: &str) -> bool {
    file.contains(".js")
        || file.contains(".mjs")
        || file.contains(".cjs")
        || file.contains(".ts")
        || file.starts_with("node:")
}

fn norm_java_line(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix("at ")?;
    let (method, paren) = rest.split_once('(')?;
    let file = paren.trim_end_matches(')').split(':').next()?.trim();
    if file.ends_with(".java") || file == "Unknown Source" || file == "Native Method" {
        return Some(format!("at {method}({file})"));
    }
    None
}

fn norm_php_line(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix('#')?;
    let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit()).trim();
    if rest.starts_with("{main}") {
        return Some("{main}".into());
    }
    let (file, after) = rest.split_once('(')?;
    if !file.contains(".php") {
        return None;
    }
    let func = after
        .split_once(')')
        .map(|(_, tail)| tail.trim().trim_start_matches(':').trim())
        .unwrap_or("");
    Some(format!("{file}: {func}"))
}

fn norm_dotnet_line(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix("at ")?;
    let (method, file) = rest.split_once(" in ")?;
    if file.contains(":line ") || file.contains(".cs") {
        return Some(format!("at {}", method.trim()));
    }
    None
}

fn norm_go_line(line: &str) -> Option<String> {
    let t = line.trim();
    if t.starts_with("goroutine ") {
        return None;
    }
    if t.contains(".go:") {
        let cut = t.split(" +0x").next().unwrap_or(t);
        return Some(strip_line_col(cut));
    }
    if t.contains('(') && (t.contains("0x") || t.ends_with("(...)")) {
        let func = t.split('(').next().unwrap_or(t).trim();
        if func.contains('.') {
            return Some(func.to_string());
        }
    }
    None
}

fn strip_line_col(s: &str) -> String {
    let mut head = s.trim();
    for _ in 0..2 {
        if let Some(i) = head.rfind(':') {
            let tail = &head[i + 1..];
            if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
                head = &head[..i];
                continue;
            }
        }
        break;
    }
    head.to_string()
}

fn title_line(language: &str, text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let picked = match language {
        "python" => lines
            .iter()
            .rev()
            .find(|l| !l.starts_with("File ") && !l.starts_with('^'))
            .copied(),
        "go" => lines
            .iter()
            .find(|l| l.contains("panic:") || l.contains("Error"))
            .copied(),
        _ => lines.first().copied(),
    };
    picked.unwrap_or(text).to_string()
}

fn first_line(text: &str) -> &str {
    text.lines().find(|l| !l.trim().is_empty()).unwrap_or(text)
}

pub fn strip_ids(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if looks_like_uuid(&chars, i) {
            out.push('?');
            i += 36;
            continue;
        }
        if chars[i] == '0' && i + 1 < chars.len() && chars[i + 1] == 'x' {
            let start = i;
            i += 2;
            while i < chars.len() && chars[i].is_ascii_hexdigit() {
                i += 1;
            }
            if i - start > 2 {
                out.push('?');
                continue;
            }
            i = start;
        }
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i - start >= 5 {
                out.push('?');
            } else {
                for c in &chars[start..i] {
                    out.push(*c);
                }
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn looks_like_uuid(chars: &[char], i: usize) -> bool {
    if i + 36 > chars.len() {
        return false;
    }
    let positions = [8, 13, 18, 23];
    for (n, ch) in chars[i..i + 36].iter().enumerate() {
        if positions.contains(&n) {
            if *ch != '-' {
                return false;
            }
        } else if !ch.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn is_starter(line: &LogSnippet) -> bool {
    if line.level == "error" || line.level == "fatal" {
        return true;
    }
    let raw = line.raw.trim_start();
    raw.contains("Traceback (most recent call last)")
        || raw.starts_with("panic:")
        || raw.contains("goroutine ")
        || raw.starts_with("PHP Fatal")
        || raw.contains("Exception")
}

fn is_continuation(raw: &str) -> bool {
    let t = raw.trim_end();
    if t.is_empty() {
        return false;
    }
    raw.starts_with(' ')
        || raw.starts_with('\t')
        || t.starts_with("at ")
        || t.starts_with("Caused by")
        || t.starts_with("...")
        || t.starts_with("File \"")
        || t.starts_with('#')
        || t.starts_with("goroutine ")
        || t.contains(".go:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_line_numbers_do_not_change_fingerprint() {
        let a = fingerprint_text(
            "Error: boom\n    at handler (/app/src/index.js:20:15)\n    at main (/app/src/index.js:40:3)",
        )
        .unwrap();
        let b = fingerprint_text(
            "Error: boom id 123456\n    at handler (/app/src/index.js:99:2)\n    at main (/app/src/index.js:4:1)",
        )
        .unwrap();
        assert_eq!(a.language, "node");
        assert_eq!(a.fingerprint, b.fingerprint);
        assert!(a.normalized_stack.contains("index.js"));
        assert!(!a.normalized_stack.contains(":20"));
    }

    #[test]
    fn python_go_java_php_dotnet_fixtures() {
        let py = fingerprint_text(
            "Traceback (most recent call last):\n  File \"/app/main.py\", line 10, in handle\n    raise ValueError(\"nope\")\nValueError: nope 123456",
        )
        .unwrap();
        assert_eq!(py.language, "python");
        assert!(py.normalized_stack.contains("in handle"));
        assert!(!py.normalized_stack.contains("line 10"));
        assert!(py.title.contains("ValueError"));
        assert!(!py.title.contains("123456"));

        let go = fingerprint_text(
            "panic: runtime error: invalid memory address\n\ngoroutine 1 [running]:\nmain.serveHTTP(0xc00014a000)\n\t/app/main.go:42 +0x1a5",
        )
        .unwrap();
        assert_eq!(go.language, "go");
        assert!(go.normalized_stack.contains("/app/main.go"));
        assert!(!go.normalized_stack.contains("+0x"));
        assert!(go.normalized_stack.contains("main.serveHTTP"));

        let java = fingerprint_text(
            "java.lang.NullPointerException: boom\n\tat com.example.App.handle(App.java:22)\n\tat com.example.App.main(App.java:8)",
        )
        .unwrap();
        assert_eq!(java.language, "java");
        assert!(java.normalized_stack.contains("App.handle(App.java)"));
        assert!(!java.normalized_stack.contains(":22"));

        let php = fingerprint_text(
            "PHP Fatal error: Uncaught Exception: nope in /app/index.php:15\nStack trace:\n#0 /app/index.php(15): doWork()\n#1 {main}",
        )
        .unwrap();
        assert_eq!(php.language, "php");
        assert!(php.normalized_stack.contains("/app/index.php: doWork()"));

        let net = fingerprint_text(
            "System.InvalidOperationException: nope\n   at MyApp.Worker.Run() in /app/Worker.cs:line 30\n   at MyApp.Program.Main() in /app/Program.cs:line 12",
        )
        .unwrap();
        assert_eq!(net.language, "dotnet");
        assert!(net.normalized_stack.contains("at MyApp.Worker.Run()"));
        assert!(!net.normalized_stack.contains("line 30"));

        let other = fingerprint_text(&java.normalized_stack.replace("handle", "other")).unwrap();
        assert_ne!(java.fingerprint, other.fingerprint);
    }

    #[test]
    fn groups_multiline_log_error() {
        let lines = vec![
            LogSnippet {
                app_id: Some("api".into()),
                level: "error".into(),
                raw: "Error: boom".into(),
                ts_ms: 10,
            },
            LogSnippet {
                app_id: Some("api".into()),
                level: "unknown".into(),
                raw: "    at handler (/app/a.js:1:1)".into(),
                ts_ms: 11,
            },
            LogSnippet {
                app_id: Some("api".into()),
                level: "info".into(),
                raw: "listening".into(),
                ts_ms: 12,
            },
        ];
        let drafts = drafts_from_log_lines(&lines);
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].app_id.as_deref(), Some("api"));
        assert_eq!(drafts[0].language, "node");
        assert_eq!(drafts[0].ts_ms, 10);
    }
}

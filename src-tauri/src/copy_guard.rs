use std::fs;
use std::path::Path;

#[test]
fn user_facing_strings_have_no_semicolon_space() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut bad = Vec::new();
    for dir in [root.join("src"), root.join("..").join("src")] {
        walk(&dir, &mut bad);
    }
    assert!(
        bad.is_empty(),
        "user-facing text contains \"; \":\n{}",
        bad.join("\n")
    );
}

fn walk(dir: &Path, bad: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "node_modules" || name == "target" {
                continue;
            }
            walk(&path, bad);
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "rs" | "ts" | "tsx") {
            continue;
        }
        if path.ends_with("copy_guard.rs") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let ts = matches!(ext, "ts" | "tsx");
        for (line_no, content) in string_literals(&text, ts) {
            if content.contains("; ") && !shell_or_code(&content) && !only_html_entities(&content) {
                bad.push(format!(
                    "{}:{}: {}",
                    path.display(),
                    line_no,
                    content.chars().take(140).collect::<String>()
                ));
            }
        }
    }
}

fn only_html_entities(value: &str) -> bool {
    let mut rest = value;
    while let Some(idx) = rest.find("; ") {
        let before = &rest[..idx];
        let Some(amp) = before.rfind('&') else {
            return false;
        };
        let entity = &before[amp..];
        if !matches!(entity, "&amp" | "&lt" | "&gt" | "&quot" | "&nbsp" | "&#39") {
            return false;
        }
        rest = &rest[idx + 2..];
    }
    true
}

fn shell_or_code(value: &str) -> bool {
    const MARKERS: &[&str] = &[
        "2>/dev/null",
        ">/dev/null",
        "printf",
        "command -v",
        "&&",
        "||",
        "fi;",
        " then",
        "echo ",
        "grep ",
        "awk",
        "sed ",
        "$(",
        "${",
        "#!/",
        "import ",
        "export ",
        "const ",
        "SHOW ",
        "SELECT ",
        "Content-Type",
        "<!DOCTYPE",
        "margin:",
        "max-age",
        "function ",
        "charset=",
    ];
    MARKERS.iter().any(|marker| value.contains(marker))
}

fn string_literals(text: &str, ts: bool) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1usize;
    while i < chars.len() {
        if chars[i] == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if chars[i] == 'r' && chars.get(i + 1) == Some(&'#') {
            let mut hashes = 0;
            let mut k = i + 1;
            while chars.get(k) == Some(&'#') {
                hashes += 1;
                k += 1;
            }
            if chars.get(k) == Some(&'"') {
                let start_line = line;
                let mut j = k + 1;
                let mut content = String::new();
                while j < chars.len() {
                    if chars[j] == '"'
                        && chars.get(j + 1..j + 1 + hashes).map(|s| s.iter().all(|c| *c == '#')).unwrap_or(false)
                    {
                        out.push((start_line, content));
                        i = j + 1 + hashes;
                        break;
                    }
                    if chars[j] == '\n' {
                        line += 1;
                    }
                    content.push(chars[j]);
                    j += 1;
                }
                if j >= chars.len() {
                    break;
                }
                continue;
            }
        }
        let quote = chars[i];
        let is_string = quote == '"' || (ts && (quote == '`' || quote == '\''));
        if is_string {
            let start_line = line;
            let mut j = i + 1;
            let mut content = String::new();
            let mut closed = false;
            while j < chars.len() {
                if chars[j] == '\\' && quote != '`' && j + 1 < chars.len() {
                    content.push(chars[j + 1]);
                    if chars[j + 1] == '\n' {
                        line += 1;
                    }
                    j += 2;
                    continue;
                }
                if quote == '`' && chars[j] == '$' && chars.get(j + 1) == Some(&'{') {
                    j += 2;
                    let mut depth = 1;
                    while j < chars.len() && depth > 0 {
                        if chars[j] == '{' {
                            depth += 1;
                        }
                        if chars[j] == '}' {
                            depth -= 1;
                        }
                        if chars[j] == '\n' {
                            line += 1;
                        }
                        j += 1;
                    }
                    continue;
                }
                if chars[j] == quote {
                    out.push((start_line, content));
                    i = j + 1;
                    closed = true;
                    break;
                }
                if chars[j] == '\n' {
                    line += 1;
                }
                content.push(chars[j]);
                j += 1;
            }
            if !closed {
                break;
            }
            continue;
        }
        i += 1;
    }
    out
}

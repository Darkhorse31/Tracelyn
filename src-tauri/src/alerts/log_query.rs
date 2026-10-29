//! Log-monitor matcher. Same terms as the explorer query in `src/lib/log-query.ts`:
//! phrases, `field:value`, numeric compares, and leading `-` to exclude.

#[derive(Debug, Clone)]
pub struct LogSample {
    pub raw: String,
    pub level: String,
    pub message: String,
    pub app: String,
    pub source: String,
    pub status: Option<f64>,
    pub trace_id: String,
    pub span_id: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
enum Term {
    Phrase { text: String, neg: bool },
    Field { field: String, value: String, neg: bool },
    Cmp { field: String, op: String, value: f64, neg: bool },
}

#[derive(Debug, Clone)]
pub struct ParsedQuery {
    terms: Vec<Term>,
}

pub fn parse_log_query(input: &str) -> ParsedQuery {
    let head = input.split('|').next().unwrap_or("");
    let mut terms = Vec::new();
    for token in tokenize(head) {
        if let Some(term) = parse_token(&token) {
            terms.push(term);
        }
    }
    ParsedQuery { terms }
}

pub fn line_matches(query: &ParsedQuery, line: &LogSample) -> bool {
    query.terms.iter().all(|term| term_matches(term, line))
}

pub fn count_matches(query: &ParsedQuery, lines: &[LogSample], since_ms: i64) -> usize {
    lines
        .iter()
        .filter(|line| line.ts_ms >= since_ms && line_matches(query, line))
        .count()
}

fn tokenize(input: &str) -> Vec<String> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let quoted = chars[i] == '"' || (chars[i] == '-' && chars.get(i + 1) == Some(&'"'));
        if quoted {
            let start = if chars[i] == '-' { i + 1 } else { i };
            if let Some(rel) = chars[start + 1..].iter().position(|c| *c == '"') {
                let end = start + 1 + rel;
                out.push(chars[i..=end].iter().collect());
                i = end + 1;
                continue;
            }
            out.push(chars[i..].iter().collect());
            break;
        }
        let mut j = i;
        while j < chars.len() && chars[j] != ' ' {
            j += 1;
        }
        out.push(chars[i..j].iter().collect());
        i = j;
    }
    out
}

fn parse_token(token: &str) -> Option<Term> {
    let mut neg = false;
    let mut body = token;
    if let Some(rest) = token.strip_prefix('-') {
        if !rest.is_empty() && !rest.starts_with('-') {
            neg = true;
            body = rest;
        }
    }
    if body.starts_with('"') && body.ends_with('"') && body.len() >= 2 {
        let text = body[1..body.len() - 1].to_string();
        return if text.is_empty() {
            None
        } else {
            Some(Term::Phrase { text, neg })
        };
    }
    if let Some(term) = parse_cmp(body, neg) {
        return Some(term);
    }
    if let Some((field, value)) = body.split_once(':') {
        if !value.is_empty() && field.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            return Some(Term::Field {
                field: field.to_ascii_lowercase(),
                value: value.to_string(),
                neg,
            });
        }
    }
    if body.is_empty() {
        return None;
    }
    Some(Term::Phrase {
        text: body.to_string(),
        neg,
    })
}

fn parse_cmp(body: &str, neg: bool) -> Option<Term> {
    for op in [">=", "<=", ">", "<", "="] {
        if let Some((field, value)) = body.split_once(op) {
            if field.is_empty() || !field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            if let Ok(number) = value.trim().parse::<f64>() {
                return Some(Term::Cmp {
                    field: field.to_ascii_lowercase(),
                    op: op.to_string(),
                    value: number,
                    neg,
                });
            }
        }
    }
    None
}

fn term_matches(term: &Term, line: &LogSample) -> bool {
    let ok = match term {
        Term::Phrase { text, .. } => line.raw.to_ascii_lowercase().contains(&text.to_ascii_lowercase()),
        Term::Cmp { field, op, value, .. } => field_number(field, line)
            .is_some_and(|got| compare(op, got, *value)),
        Term::Field { field, value, .. } => {
            let text = field_text(field, line).to_ascii_lowercase();
            let want = value.to_ascii_lowercase();
            if field == "level" {
                text == want
            } else {
                text.contains(&want)
            }
        }
    };
    match term {
        Term::Phrase { neg, .. } | Term::Field { neg, .. } | Term::Cmp { neg, .. } => {
            if *neg { !ok } else { ok }
        }
    }
}

fn compare(op: &str, got: f64, want: f64) -> bool {
    match op {
        ">" => got > want,
        ">=" => got >= want,
        "<" => got < want,
        "<=" => got <= want,
        _ => (got - want).abs() < f64::EPSILON,
    }
}

fn field_number(field: &str, line: &LogSample) -> Option<f64> {
    if field == "status" || field == "http_status" {
        return line.status;
    }
    None
}

fn field_text(field: &str, line: &LogSample) -> String {
    match field {
        "level" => line.level.clone(),
        "app" | "app_id" => line.app.clone(),
        "source" | "source_id" => line.source.clone(),
        "status" | "http_status" => line.status.map(|n| n.to_string()).unwrap_or_default(),
        "message" => {
            if line.message.is_empty() {
                line.raw.clone()
            } else {
                line.message.clone()
            }
        }
        "trace_id" | "traceid" => line.trace_id.clone(),
        "span_id" | "spanid" => line.span_id.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(raw: &str, level: &str, status: Option<f64>) -> LogSample {
        LogSample {
            raw: raw.into(),
            level: level.into(),
            message: raw.into(),
            app: "api".into(),
            source: "stdout".into(),
            status,
            trace_id: "abc".into(),
            span_id: "def".into(),
            ts_ms: 1_000,
        }
    }

    #[test]
    fn query_matches_phrase_field_compare_and_negation() {
        let err = line("payment failed", "error", Some(500.0));
        let ok = line("payment ok", "info", Some(200.0));
        let query = parse_log_query(r#"payment level:error status>=500 -"ok""#);
        assert!(line_matches(&query, &err));
        assert!(!line_matches(&query, &ok));
        assert_eq!(count_matches(&query, &[err, ok], 0), 1);
    }
}

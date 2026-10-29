//! Self-contained HTML and a small text PDF. No external assets, no secrets.

use super::logic::clip_chars;
use super::types::{DigestSpec, Incident};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ReportDoc {
    pub title: String,
    pub subtitle: String,
    pub rows: Vec<String>,
    pub series_name: String,
    pub series: Vec<(i64, f64)>,
}

pub fn render_html(doc: &ReportDoc) -> String {
    let mut rows = String::new();
    if doc.rows.is_empty() {
        rows.push_str("<p>No events in this report.</p>");
    } else {
        rows.push_str("<ol>");
        for row in &doc.rows {
            rows.push_str("<li>");
            rows.push_str(&esc(row));
            rows.push_str("</li>");
        }
        rows.push_str("</ol>");
    }
    let chart = sparkline(&doc.series_name, &doc.series);
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
  body {{ margin: 0; padding: 32px; color: #0f172a; background: #fff; font: 15px/1.5 "Segoe UI", sans-serif; }}
  h1 {{ font-size: 22px; margin: 0 0 4px; }}
  p.sub {{ color: #334155; margin: 0 0 20px; }}
  ol {{ padding-left: 1.2rem; }}
  li {{ margin: 0.25rem 0; }}
  svg {{ width: 100%; max-width: 640px; height: auto; background: #f8fafc; border: 1px solid #e2e8f0; }}
  @media print {{ body {{ padding: 0; }} }}
</style>
</head>
<body>
<h1>{title}</h1>
<p class="sub">{subtitle}</p>
{chart}
{rows}
</body>
</html>
"#,
        title = esc(&doc.title),
        subtitle = esc(&doc.subtitle),
    )
}

fn esc(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            _ => c.to_string(),
        })
        .collect()
}

fn sparkline(name: &str, points: &[(i64, f64)]) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let min = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let span = (max - min).max(1e-9);
    let last = points.len() - 1;
    let coords: Vec<String> = points
        .iter()
        .enumerate()
        .map(|(i, point)| {
            let x = i as f64 / last as f64 * 600.0;
            let y = 78.0 - ((point.1 - min) / span) * 68.0;
            format!("{x:.1},{y:.1}")
        })
        .collect();
    format!(
        "<figure><svg viewBox=\"0 0 600 90\" role=\"img\" aria-label=\"{name}\"><title>{name}</title><polyline fill=\"none\" stroke=\"#2563eb\" stroke-width=\"1.75\" points=\"{pts}\" /></svg><figcaption>{name}</figcaption></figure>",
        name = esc(name),
        pts = coords.join(" ")
    )
}

pub fn render_markdown(doc: &ReportDoc) -> String {
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(&doc.title);
    out.push_str("\n\n");
    out.push_str(&doc.subtitle);
    out.push_str("\n\n## Timeline\n\n");
    if doc.rows.is_empty() {
        out.push_str("No events in this window.\n");
    } else {
        for row in &doc.rows {
            out.push_str("- ");
            out.push_str(row);
            out.push('\n');
        }
    }
    out
}

pub fn render_pdf(doc: &ReportDoc) -> Vec<u8> {
    let mut lines = Vec::new();
    lines.push(doc.title.clone());
    lines.push(doc.subtitle.clone());
    lines.push(String::new());
    if doc.series.len() >= 2 {
        let last = doc.series.last().map(|p| p.1).unwrap_or(0.0);
        lines.push(format!(
            "{}: {} points, latest {:.3}",
            doc.series_name,
            doc.series.len(),
            last
        ));
        lines.push(String::new());
    }
    if doc.rows.is_empty() {
        lines.push("No events in this report.".into());
    }
    for row in &doc.rows {
        for part in wrap(row, 90) {
            lines.push(part);
        }
    }
    pdf_from_lines(&lines)
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let extra = if current.is_empty() { 0 } else { 1 };
        if current.chars().count() + extra + word.chars().count() > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn pdf_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '\\' | '(' | ')' => {
                out.push('\\');
                out.push(ch);
            }
            c if c.is_ascii() && !c.is_control() => out.push(c),
            _ => out.push('?'),
        }
    }
    out
}

fn pdf_from_lines(lines: &[String]) -> Vec<u8> {
    let mut pages: Vec<Vec<(i32, String)>> = Vec::new();
    let mut page: Vec<(i32, String)> = Vec::new();
    let mut y = 800;
    for line in lines.iter().take(800) {
        if y < 48 {
            pages.push(std::mem::take(&mut page));
            y = 800;
        }
        page.push((y, pdf_escape(line)));
        y -= 14;
    }
    if page.is_empty() {
        page.push((800, String::new()));
    }
    pages.push(page);
    if pages.len() > 30 {
        pages.truncate(30);
    }

    let mut objects: Vec<String> = Vec::new();
    objects.push(String::new());
    let page_count = pages.len();
    let mut kids = Vec::new();
    for i in 0..page_count {
        kids.push(format!("{} 0 R", 4 + i * 2));
    }
    objects.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    objects.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {page_count} >>",
        kids.join(" ")
    ));
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    for (index, page) in pages.iter().enumerate() {
        let page_id = 4 + index * 2;
        let content_id = page_id + 1;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents {content_id} 0 R /Resources << /Font << /F1 3 0 R >> >> >>"
        ));
        let mut stream = String::from("BT /F1 11 Tf\n");
        for (y, text) in page {
            stream.push_str(&format!("1 0 0 1 48 {y} Tm ({text}) Tj\n"));
        }
        stream.push_str("ET");
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ));
    }

    let mut buf = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = vec![0u32];
    for (i, object) in objects.iter().enumerate().skip(1) {
        offsets.push(buf.len() as u32);
        let header = format!("{i} 0 obj\n{object}\nendobj\n");
        buf.extend_from_slice(header.as_bytes());
    }
    let xref_at = buf.len();
    let total = objects.len();
    buf.extend_from_slice(format!("xref\n0 {total}\n").as_bytes());
    buf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        buf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(
        format!("trailer\n<< /Size {total} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n").as_bytes(),
    );
    buf
}

pub fn format_utc(ts_ms: i64) -> String {
    let secs = ts_ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        sod / 3600,
        (sod % 3600) / 60,
        sod % 60
    )
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    (y as i32, m as u32, d)
}

pub fn digest_message(spec: &DigestSpec, incidents: &[Incident], anomaly_count: usize, since_ms: i64) -> (String, String) {
    let subject = format!("Tracelyn digest: {}", spec.name);
    let mut body = format!(
        "{}\nWindow starts {}\nIncidents: {}\nAnomalies: {}\n\n",
        spec.name,
        format_utc(since_ms),
        incidents.len(),
        anomaly_count
    );
    if incidents.is_empty() {
        body.push_str("No incidents in this window.\n");
    }
    for incident in incidents.iter().take(20) {
        body.push_str(&format!(
            "- [{}] {} ({})\n",
            incident.severity,
            clip_chars(&incident.title, 120),
            incident.status
        ));
    }
    (subject, body)
}

const SECRET_KEYS: &[&str] = &[
    "password",
    "secret",
    "token",
    "bottoken",
    "webhookurl",
    "passphrase",
    "apikey",
];

pub fn contains_secret_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, child)| {
            SECRET_KEYS.contains(&key.to_ascii_lowercase().as_str()) || contains_secret_key(child)
        }),
        Value::Array(items) => items.iter().any(contains_secret_key),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ReportDoc {
        ReportDoc {
            title: "CPU incident".into(),
            subtitle: "web-1 · critical · open".into(),
            rows: vec!["2026-10-26 alert: cpu high".into(), "note: paging the on-call".into()],
            series_name: "cpu.percent".into(),
            series: vec![(0, 10.0), (60_000, 40.0), (120_000, 12.0)],
        }
    }

    #[test]
    fn html_is_self_contained() {
        let html = render_html(&sample());
        assert!(html.contains("CPU incident"));
        assert!(html.contains("<polyline"));
        assert!(html.contains("paging the on-call"));
        let markdown = render_markdown(&sample());
        assert!(markdown.starts_with("# CPU incident\n"));
        assert!(markdown.contains("## Timeline"));
        assert!(markdown.contains("- 2026-10-26 alert: cpu high"));
        assert!(!html.contains("<script"));
        assert!(!html.contains("https://"));
        let hostile = ReportDoc {
            title: "<script>".into(),
            subtitle: String::new(),
            rows: vec!["a & b".into()],
            series_name: String::new(),
            series: vec![],
        };
        let escaped = render_html(&hostile);
        assert!(escaped.contains("&lt;script&gt;"));
        assert!(escaped.contains("a &amp; b"));
        assert!(!escaped.contains("<script>"));
    }

    #[test]
    fn pdf_starts_with_header_and_keeps_title() {
        let bytes = render_pdf(&sample());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("CPU incident"));
        assert!(text.contains("%%EOF"));
        assert!(text.contains("xref"));
    }

    #[test]
    fn epoch_formats_as_utc() {
        assert_eq!(format_utc(0), "1970-01-01 00:00:00 UTC");
    }

    #[test]
    fn secret_keys_are_detected() {
        let clean = serde_json::json!({"servers": [{"name": "a", "host": "h"}]});
        assert!(!contains_secret_key(&clean));
        let dirty = serde_json::json!({"password": "nope"});
        assert!(contains_secret_key(&dirty));
    }
}

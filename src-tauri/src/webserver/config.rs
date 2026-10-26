use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebServerKind {
    Nginx,
    Apache,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFormatInfo {
    pub name: String,
    pub format_string: String,
    pub has_request_time: bool,
    pub has_upstream_time: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingField {
    pub present: bool,
    pub directive_line: Option<String>,
    pub suggested_line: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NginxConfigInfo {
    pub access_log_paths: Vec<String>,
    pub error_log_paths: Vec<String>,
    pub log_formats: Vec<LogFormatInfo>,
    pub access_log_format_name: Option<String>,
    pub request_timing: TimingField,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApacheConfigInfo {
    pub access_log_paths: Vec<String>,
    pub error_log_paths: Vec<String>,
    pub log_formats: Vec<LogFormatInfo>,
    pub request_timing: TimingField,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParsedWebConfig {
    Nginx(NginxConfigInfo),
    Apache(ApacheConfigInfo),
}

pub fn suggest_nginx_timing_line(format_name: &str) -> String {
    format!(
        "log_format {name} '$remote_addr - $remote_user [$time_local] \"$request\" $status $body_bytes_sent \"$http_referer\" \"$http_user_agent\" $request_time $upstream_response_time';\naccess_log /var/log/nginx/access.log {name};\nnginx -t\nsystemctl reload nginx",
        name = format_name
    )
}

pub fn suggest_apache_timing_line() -> String {
    r#"LogFormat "%h %l %u %t \"%r\" %>s %b \"%{Referer}i\" \"%{User-Agent}i\" %D" combined_timing
CustomLog ${APACHE_LOG_DIR}/access.log combined_timing
apache2ctl configtest
systemctl reload apache2"#
        .to_string()
}

fn strip_comments(line: &str) -> String {
    let mut out = String::new();
    let mut in_single = false;
    for ch in line.chars() {
        if ch == '\'' && !in_single {
            in_single = true;
            out.push(ch);
            continue;
        }
        if ch == '\'' && in_single {
            in_single = false;
            out.push(ch);
            continue;
        }
        if ch == '#' && !in_single {
            break;
        }
        out.push(ch);
    }
    out.trim().to_string()
}

fn nginx_log_format_has_timing(format_str: &str) -> (bool, bool) {
    let req = format_str.contains("$request_time");
    let upstream = format_str.contains("$upstream_response_time");
    (req, upstream)
}

pub fn parse_nginx_config(text: &str) -> ParsedWebConfig {
    let mut access_log_paths = Vec::new();
    let mut error_log_paths = Vec::new();
    let mut log_formats: Vec<LogFormatInfo> = Vec::new();
    let mut access_log_format_name: Option<String> = None;
    let mut timing_directive_line: Option<String> = None;

    let mut i = 0;
    let lines: Vec<&str> = text.lines().collect();
    while i < lines.len() {
        let raw = strip_comments(lines[i]);
        let raw = raw.trim();
        if raw.is_empty() {
            i += 1;
            continue;
        }
        if raw.starts_with("log_format ") {
            let rest = raw["log_format ".len()..].trim();
            let name_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let name = rest[..name_end].to_string();
            let mut format_str = rest[name_end..].trim().to_string();
            if format_str.starts_with('\'') || format_str.starts_with('"') {
                let quote = format_str.chars().next().unwrap_or('\'');
                while !format_str.ends_with(quote) && i + 1 < lines.len() {
                    i += 1;
                    format_str.push(' ');
                    format_str.push_str(strip_comments(lines[i]).trim());
                }
            }
            format_str = format_str
                .trim()
                .trim_matches(|c| c == '\'' || c == '"')
                .trim_end_matches(';')
                .trim_matches(|c| c == '\'' || c == '"')
                .to_string();
            let (has_request_time, has_upstream_time) = nginx_log_format_has_timing(&format_str);
            if !has_request_time {
                timing_directive_line = Some(lines[i].trim().to_string());
            }
            log_formats.push(LogFormatInfo {
                name,
                format_string: format_str,
                has_request_time,
                has_upstream_time,
            });
        } else if raw.starts_with("access_log ") {
            let parts: Vec<&str> = raw["access_log ".len()..]
                .split_whitespace()
                .collect();
            if !parts.is_empty() {
                access_log_paths.push(parts[0].trim_end_matches(';').to_string());
                if parts.len() >= 2 && !parts[1].starts_with("buffer=") {
                    let fmt = parts[1].trim_end_matches(';').trim();
                    if !fmt.is_empty() {
                        access_log_format_name = Some(fmt.to_string());
                    }
                }
            }
        } else if raw.starts_with("error_log ") {
            let path = raw["error_log ".len()..]
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches(';');
            if !path.is_empty() {
                error_log_paths.push(path.to_string());
            }
        }
        i += 1;
    }

    let active_format = access_log_format_name.as_deref().or_else(|| {
        log_formats.iter().find(|f| f.name == "combined").map(|f| f.name.as_str())
    });
    let has_timing = active_format
        .and_then(|n| log_formats.iter().find(|f| f.name == n))
        .map(|f| f.has_request_time)
        .unwrap_or(false)
        || (log_formats.len() == 1 && log_formats[0].has_request_time);

    let suggested = if !has_timing {
        active_format.map(suggest_nginx_timing_line)
    } else {
        None
    };

    ParsedWebConfig::Nginx(NginxConfigInfo {
        access_log_paths,
        error_log_paths,
        log_formats,
        access_log_format_name,
        request_timing: TimingField {
            present: has_timing,
            directive_line: if has_timing {
                None
            } else {
                timing_directive_line
            },
            suggested_line: suggested,
        },
    })
}

fn apache_format_has_timing(format_str: &str) -> bool {
    format_str.contains("%D") || format_str.contains("%T")
}

pub fn parse_apache_config(text: &str) -> ParsedWebConfig {
    let mut access_log_paths = Vec::new();
    let mut error_log_paths = Vec::new();
    let mut log_formats: Vec<LogFormatInfo> = Vec::new();
    let mut timing_directive_line: Option<String> = None;
    let mut active_format_has_timing = false;

    for line in text.lines() {
        let stripped = strip_comments(line);
        let raw = stripped.trim();
        if raw.is_empty() {
            continue;
        }
        let lower = raw.to_lowercase();
        if lower.starts_with("logformat ") || lower.starts_with("customlog ") {
            if lower.starts_with("logformat ") {
                let rest = raw.splitn(3, '"').collect::<Vec<_>>();
                if rest.len() >= 2 {
                    let format_str = rest[1].to_string();
                    let name = rest
                        .get(2)
                        .map(|s| s.trim().trim_matches('"').trim_end_matches(';').to_string())
                        .unwrap_or_else(|| "unnamed".into());
                    let has = apache_format_has_timing(&format_str);
                    if !has {
                        timing_directive_line = Some(line.trim().to_string());
                    }
                    log_formats.push(LogFormatInfo {
                        name,
                        format_string: format_str,
                        has_request_time: has,
                        has_upstream_time: false,
                    });
                }
            }
            if lower.starts_with("customlog ") {
                let parts: Vec<&str> = raw.split_whitespace().collect();
                if parts.len() >= 2 {
                    access_log_paths.push(parts[1].trim_end_matches(';').to_string());
                }
                if let Some(fmt) = parts.last() {
                    let fmt = fmt.trim_end_matches(';');
                    if let Some(lf) = log_formats.iter().find(|f| f.name == fmt) {
                        active_format_has_timing = lf.has_request_time;
                    }
                }
            }
        } else if lower.starts_with("errorlog ") {
            let path = raw.split_whitespace().nth(1).unwrap_or("").trim_end_matches(';');
            if !path.is_empty() {
                error_log_paths.push(path.to_string());
            }
        }
    }

    let suggested = if !active_format_has_timing {
        Some(suggest_apache_timing_line())
    } else {
        None
    };

    ParsedWebConfig::Apache(ApacheConfigInfo {
        access_log_paths,
        error_log_paths,
        log_formats,
        request_timing: TimingField {
            present: active_format_has_timing,
            directive_line: if active_format_has_timing {
                None
            } else {
                timing_directive_line
            },
            suggested_line: suggested,
        },
    })
}

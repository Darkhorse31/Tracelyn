//! Read-only import parsing. This module never opens keys or executes SSH directives.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportServer {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub key_path: Option<String>,
    pub labels: Vec<String>,
    pub proxy_jump: Option<String>,
    pub issues: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub servers: Vec<ImportServer>,
    pub warnings: Vec<String>,
}

fn words(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && chars
                .peek()
                .is_some_and(|n| Some(*n) == quote || *n == '#' || *n == ' ')
        {
            word.push(chars.next().unwrap());
        } else if Some(c) == quote {
            quote = None;
        } else if quote.is_none() && matches!(c, '\'' | '"') {
            quote = Some(c);
        } else if quote.is_none() && c == '#' {
            break;
        } else if quote.is_none() && c.is_whitespace() {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
        } else {
            word.push(c);
        }
    }
    if quote.is_some() {
        return Err("Unclosed quote".into());
    }
    if !word.is_empty() {
        out.push(word);
    }
    Ok(out)
}

fn matches_host(patterns: &[String], alias: &str) -> bool {
    let mut positive = false;
    for pattern in patterns {
        let (negative, raw) = pattern
            .strip_prefix('!')
            .map(|p| (true, p))
            .unwrap_or((false, pattern));
        let regex = format!(
            "(?i)^{}$",
            regex::escape(raw).replace("\\*", ".*").replace("\\?", ".")
        );
        let matched = regex::Regex::new(&regex).is_ok_and(|r| r.is_match(alias));
        if matched && negative {
            return false;
        }
        positive |= matched;
    }
    positive
}

fn port(raw: &str, issues: &mut Vec<String>) -> u16 {
    if raw.is_empty() || !raw.chars().all(|c| c.is_ascii_digit()) {
        issues.push("Port must be a number from 1 to 65535".into());
        return 22;
    }
    match raw.parse::<u16>() {
        Ok(n) if n > 0 => n,
        _ => {
            issues.push("Port must be a number from 1 to 65535".into());
            22
        }
    }
}

fn validate(row: &mut ImportServer) {
    if row.host.trim().is_empty()
        || row.host.chars().any(char::is_whitespace)
        || row.host.starts_with('-')
    {
        row.issues
            .push("A host name or IP address is required".into());
    }
    if row.username.trim().is_empty() || row.username.chars().any(char::is_whitespace) {
        row.issues.push("An SSH username is required".into());
    }
    if row
        .proxy_jump
        .as_deref()
        .is_some_and(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none"))
    {
        row.issues
            .push("ProxyJump is not supported yet. Use a directly reachable host".into());
    }
    if row.host.contains('%')
        || row
            .key_path
            .as_deref()
            .is_some_and(|p| p.contains('%') || p.starts_with('~'))
    {
        row.issues
            .push("Resolve SSH path and host tokens before importing".into());
    }
}

pub fn parse_ssh_config(text: &str, home: &str, default_user: &str) -> ImportPreview {
    let mut result = ImportPreview::default();
    let mut blocks: Vec<(Vec<String>, HashMap<String, String>)> =
        vec![(vec!["*".into()], HashMap::new())];
    let mut aliases = Vec::new();
    let mut seen = HashSet::new();
    let mut in_match = false;
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let key_end = line
            .find(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or(line.len());
        let key = line[..key_end].to_ascii_lowercase();
        let rest = line[key_end..].trim_start();
        let rest = rest.strip_prefix('=').unwrap_or(rest).trim_start();
        let values = match words(rest) {
            Ok(p) => p,
            Err(e) => {
                result.warnings.push(format!("Line {}: {e}", index + 1));
                continue;
            }
        };
        if key == "host" {
            in_match = false;
            for alias in &values {
                if !alias.contains(['*', '?', '!']) && seen.insert(alias.to_ascii_lowercase()) {
                    aliases.push(alias.clone());
                }
            }
            blocks.push((values, HashMap::new()));
        } else if key == "match" {
            in_match = true;
            result
                .warnings
                .push(format!("Line {}: Match rules were skipped", index + 1));
        } else if key == "include" {
            result.warnings.push(format!(
                "Line {}: Include files were not opened. Paste their Host entries to import them",
                index + 1
            ));
        } else if !in_match {
            blocks
                .last_mut()
                .unwrap()
                .1
                .entry(key)
                .or_insert(values.join(" "));
        }
    }
    for alias in aliases {
        let mut options = HashMap::new();
        for (patterns, values) in &blocks {
            if matches_host(patterns, &alias) {
                for (k, v) in values {
                    options.entry(k.clone()).or_insert(v.clone());
                }
            }
        }
        let host = options
            .get("hostname")
            .cloned()
            .unwrap_or_else(|| alias.clone())
            .replace("%h", &alias);
        let username = options
            .get("user")
            .cloned()
            .unwrap_or_else(|| default_user.into());
        let mut issues = Vec::new();
        let port = port(
            options.get("port").map(String::as_str).unwrap_or("22"),
            &mut issues,
        );
        let key_path = options.get("identityfile").map(|p| {
            let path = if p == "~" {
                home.to_string()
            } else if let Some(tail) = p.strip_prefix("~/") {
                format!("{home}/{tail}")
            } else {
                p.clone()
            };
            path.replace("%%", "\u{0001}")
                .replace("%d", home)
                .replace("%h", &host)
                .replace("%n", &alias)
                .replace("%r", &username)
                .replace("%p", &port.to_string())
                .replace('\u{0001}', "%")
        });
        if options
            .get("proxycommand")
            .is_some_and(|v| !v.eq_ignore_ascii_case("none"))
        {
            issues.push("ProxyCommand is not supported. Use a directly reachable host".into());
        }
        let mut row = ImportServer {
            name: alias,
            host,
            port,
            username,
            auth_type: "key".into(),
            key_path,
            labels: vec![],
            proxy_jump: options.get("proxyjump").cloned(),
            issues,
        };
        validate(&mut row);
        result.servers.push(row);
    }
    result
}

fn csv_records(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut closed = false;
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() && !closed {
            quoted = true;
        } else if c == ',' || c == '\n' || c == '\r' {
            record.push(std::mem::take(&mut field));
            closed = false;
            if c != ',' {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                if record.iter().any(|v| !v.trim().is_empty()) {
                    records.push(std::mem::take(&mut record));
                } else {
                    record.clear();
                }
            }
        } else if closed || c == '"' {
            return Err("Malformed CSV. Quotes must surround the whole field".into());
        } else {
            field.push(c);
        }
    }
    if quoted {
        return Err("Malformed CSV. A quoted field was not closed".into());
    }
    record.push(field);
    if record.iter().any(|v| !v.trim().is_empty()) {
        records.push(record);
    }
    Ok(records)
}

pub fn parse_csv(text: &str) -> Result<ImportPreview, String> {
    let records = csv_records(text)?;
    let headers: Vec<String> = records
        .first()
        .ok_or("CSV is empty")?
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();
    if headers.iter().collect::<HashSet<_>>().len() != headers.len() {
        return Err("CSV has duplicate column names".into());
    }
    if !headers.iter().any(|h| h == "host")
        || !headers.iter().any(|h| h == "username" || h == "user")
    {
        return Err("CSV needs host and username columns".into());
    }
    if headers.iter().any(|h| {
        matches!(
            h.as_str(),
            "password" | "keypassphrase" | "privatekey" | "secret"
        )
    }) {
        return Err(
            "Remove secret columns before importing. Add credentials in Edit server".into(),
        );
    }
    let mut preview = ImportPreview::default();
    let known = [
        "name",
        "host",
        "port",
        "username",
        "user",
        "authtype",
        "keypath",
        "labels",
        "proxyjump",
    ];
    for h in &headers {
        if !known.contains(&h.as_str()) {
            preview
                .warnings
                .push(format!("Unknown column ignored: {h}"));
        }
    }
    for (i, cells) in records.iter().enumerate().skip(1) {
        let get = |key: &str| {
            headers
                .iter()
                .position(|h| h == key)
                .and_then(|n| cells.get(n))
                .map(|s| s.trim())
                .unwrap_or("")
        };
        let mut issues = Vec::new();
        if cells.len() != headers.len() {
            issues.push(format!("CSV row {} has the wrong number of columns", i + 1));
        }
        let raw_port = get("port");
        let port = port(
            if raw_port.is_empty() { "22" } else { raw_port },
            &mut issues,
        );
        let auth_type = if get("authtype").is_empty() {
            "key"
        } else {
            get("authtype")
        };
        if !matches!(auth_type, "key" | "password") {
            issues.push("Authentication must be key or password".into());
        }
        let raw_labels = get("labels");
        let labels = if raw_labels.starts_with('[') {
            serde_json::from_str::<Vec<String>>(raw_labels).unwrap_or_else(|_| {
                issues.push("Labels must be a JSON list or separated by |".into());
                vec![]
            })
        } else {
            raw_labels
                .split('|')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(String::from)
                .collect()
        };
        let mut row = ImportServer {
            name: if get("name").is_empty() {
                get("host")
            } else {
                get("name")
            }
            .into(),
            host: get("host").into(),
            port,
            username: if get("username").is_empty() {
                get("user")
            } else {
                get("username")
            }
            .into(),
            auth_type: auth_type.into(),
            key_path: (!get("keypath").is_empty()).then(|| get("keypath").into()),
            labels,
            proxy_jump: (!get("proxyjump").is_empty()).then(|| get("proxyjump").into()),
            issues,
        };
        validate(&mut row);
        preview.servers.push(row);
    }
    Ok(preview)
}

pub fn export_csv(servers: &[crate::config::ServerRecord]) -> String {
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut lines = vec!["name,host,port,username,authType,keyPath,labels".to_string()];
    for s in servers {
        let auth = if s.auth_type == crate::config::AuthType::Key {
            "key"
        } else {
            "password"
        };
        let labels = serde_json::to_string(&s.labels).unwrap_or_else(|_| "[]".into());
        lines.push(
            [
                s.name.clone(),
                s.host.clone(),
                s.port.to_string(),
                s.username.clone(),
                auth.into(),
                s.key_path.clone().unwrap_or_default(),
                labels,
            ]
            .iter()
            .map(|v| quote(v))
            .collect::<Vec<_>>()
            .join(","),
        );
    }
    lines.join("\r\n") + "\r\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_aliases_inherit_first_value_and_expand_paths() {
        let preview = parse_ssh_config("Host prod mirror\n HostName=example.com\n User deploy\n Port 2222\n IdentityFile \"~/.ssh/my key\" # comment\nHost * !mirror\n User ignored\n ProxyJump bastion\nHost *.test\n HostName ignored\n", "/home/me", "me");
        assert_eq!(preview.servers.len(), 2);
        let row = &preview.servers[0];
        assert_eq!(row.host, "example.com");
        assert_eq!(row.username, "deploy");
        assert_eq!(row.port, 2222);
        assert_eq!(row.key_path.as_deref(), Some("/home/me/.ssh/my key"));
        assert_eq!(row.proxy_jump.as_deref(), Some("bastion"));
        assert!(!row.issues.is_empty());
        assert!(preview.servers[1].proxy_jump.is_none());
    }

    #[test]
    fn ssh_match_include_and_invalid_ports_are_visible() {
        let p = parse_ssh_config("Include config.d/*\nHost a\n Port 70000\nMatch exec unsafe\n User bad\nHost b\n User good\n", "/home/me", "me");
        assert_eq!(p.warnings.len(), 2);
        assert_eq!(p.servers[0].username, "me");
        assert_eq!(p.servers[1].username, "good");
        assert_eq!(p.servers[0].issues.len(), 1);
    }

    #[test]
    fn csv_quotes_bom_crlf_multiline_and_labels() {
        let p = parse_csv("\u{feff}name,host,username,port,labels\r\n\"My \"\"VPS\"\"\nwest\",example.com,deploy,2222,prod|web\r\n").unwrap();
        assert_eq!(p.servers.len(), 1);
        assert_eq!(p.servers[0].name, "My \"VPS\"\nwest");
        assert_eq!(p.servers[0].labels, vec!["prod", "web"]);
        assert_eq!(p.servers[0].port, 2222);
    }

    #[test]
    fn csv_rejects_bad_headers_secrets_and_malformed_quotes() {
        for raw in [
            "host,host,username\na,b,c",
            "host\na",
            "host,username,password\na,b,c",
            "host,username\n\"a,b",
            "host,username\n\"a\"oops,b",
        ] {
            assert!(parse_csv(raw).is_err(), "{raw}");
        }
        let p = parse_csv("host,username,port,authType\na,b,0,invalid\n").unwrap();
        assert_eq!(p.servers[0].issues.len(), 2);
    }

    #[test]
    fn shared_parser_fixtures_match_frontend() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../../src/lib/server-import.fixtures.json"))
                .unwrap();
        for case in fixtures.as_array().unwrap() {
            let preview = if case["kind"] == "ssh" {
                parse_ssh_config(case["input"].as_str().unwrap(), "/home/me", "me")
            } else {
                parse_csv(case["input"].as_str().unwrap()).unwrap()
            };
            assert_eq!(
                serde_json::to_value(&preview.servers).unwrap(),
                case["servers"],
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn export_round_trips_only_profile_fields() {
        let server = crate::config::ServerRecord {
            id: "internal-id".into(),
            name: "West, \"primary\"\n東京".into(),
            host: "example.com".into(),
            port: 2222,
            username: "deploy".into(),
            auth_type: crate::config::AuthType::Key,
            key_path: Some("C:\\my keys\\id_ed25519".into()),
            labels: vec!["a|b".into(), "with,comma".into()],
        };
        let raw = export_csv(&[server.clone()]);
        assert!(!raw.contains("internal-id"));
        assert!(!raw.contains("password"));
        let row = &parse_csv(&raw).unwrap().servers[0];
        assert_eq!(row.name, server.name);
        assert_eq!(row.key_path, server.key_path);
        assert_eq!(row.labels, server.labels);
    }
}

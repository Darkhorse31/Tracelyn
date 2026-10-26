use crate::apps::types::{LogSourceKind, LogSourceRef};

#[derive(Debug, Clone)]
pub struct ApacheVhost {
    pub server_names: Vec<String>,
    pub port: Option<u16>,
    pub conf_path: Option<String>,
    pub access_log: Option<String>,
    pub error_log: Option<String>,
}

/// Parses combined `apache2ctl -S` and vhost snippet text.
pub fn parse_apache_s_and_vhosts(text: &str) -> Vec<ApacheVhost> {
    let mut vhosts = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.contains("namevhost") || line.contains("port ") {
            let mut names = Vec::new();
            if let Some(idx) = line.find("namevhost ") {
                let rest = &line[idx + "namevhost ".len()..];
                let name = rest.split_whitespace().next().unwrap_or("");
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
            let port = line
                .split("port ")
                .nth(1)
                .and_then(|p| p.split_whitespace().next())
                .and_then(|p| p.parse().ok());
            let conf_path = line
                .split('(')
                .nth(1)
                .and_then(|p| p.split(':').next())
                .map(|p| p.trim().to_string());
            if !names.is_empty() {
                vhosts.push(ApacheVhost {
                    server_names: names,
                    port,
                    conf_path,
                    access_log: None,
                    error_log: None,
                });
            }
        }
        if line.to_lowercase().starts_with("customlog ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let path = parts[1].trim_end_matches(';').to_string();
                if let Some(last) = vhosts.last_mut() {
                    last.access_log = Some(path);
                }
            }
        }
    }
    vhosts
}

/// `grep -n CustomLog` lines: `path:line:CustomLog /var/log/x combined`
pub fn apply_apache_custom_logs(vhosts: &mut [ApacheVhost], grep_output: &str) {
    for line in grep_output.lines() {
        let mut parts = line.splitn(3, ':');
        let path = parts.next().unwrap_or("");
        let _lineno = parts.next();
        let rest = parts.next().unwrap_or("");
        let lower = rest.to_lowercase();
        if !lower.contains("customlog") && !lower.contains("errorlog") {
            continue;
        }
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        let log_path = tokens.get(1).map(|s| s.trim_end_matches(';').to_string());
        let Some(log_path) = log_path else { continue };
        for vh in vhosts.iter_mut() {
            if vh.conf_path.as_deref() == Some(path) {
                if lower.contains("errorlog") {
                    vh.error_log = Some(log_path.clone());
                } else {
                    vh.access_log = Some(log_path.clone());
                }
            }
        }
    }
}

pub fn apache_log_sources(v: &ApacheVhost, idx: usize) -> Vec<LogSourceRef> {
    let vhost_label = v
        .server_names
        .first()
        .cloned()
        .unwrap_or_else(|| format!("apache-{}", idx));
    let mut sources = Vec::new();
    if let Some(path) = &v.access_log {
        sources.push(LogSourceRef {
            id: format!("apache-access-{}", idx),
            kind: LogSourceKind::ApacheAccess,
            path: Some(path.clone()),
            container_id: None,
            vhost: Some(vhost_label.clone()),
            readable: true,
            issue: None,
            fix_hint: None,
        });
    }
    sources
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_apache_s() {
        let fixture = include_str!("../fixtures/apache_s.txt");
        let mut vhosts = parse_apache_s_and_vhosts(fixture);
        assert!(!vhosts.is_empty());
        assert_eq!(
            vhosts[0].conf_path.as_deref(),
            Some("/etc/apache2/sites-enabled/example.conf")
        );
        apply_apache_custom_logs(
            &mut vhosts,
            "/etc/apache2/sites-enabled/example.conf:3:CustomLog /var/log/apache2/example_access.log combined\n",
        );
        assert!(vhosts[0].access_log.as_deref().unwrap().contains("example_access"));
    }
}

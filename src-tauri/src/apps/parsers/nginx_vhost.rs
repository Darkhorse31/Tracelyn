use crate::apps::types::{LogSourceKind, LogSourceRef};

#[derive(Debug, Clone)]
pub struct NginxVhost {
    pub server_names: Vec<String>,
    pub listen_ports: Vec<u16>,
    pub root: Option<String>,
    pub proxy_ports: Vec<u16>,
    pub access_log: Option<String>,
    pub error_log: Option<String>,
}

pub fn parse_nginx_t_vhosts(text: &str) -> Vec<NginxVhost> {
    let mut vhosts = Vec::new();
    let mut brace = 0i32;
    let mut server_at: Option<i32> = None;
    let mut current = empty_vhost();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        let opens = trimmed.matches('{').count() as i32;
        let closes = trimmed.matches('}').count() as i32;
        if server_at.is_none() && (trimmed.starts_with("server {") || trimmed == "server{") {
            server_at = Some(brace + opens);
            current = empty_vhost();
        } else if server_at.is_some() {
            capture_directive(&mut current, trimmed);
        }
        brace += opens - closes;
        if let Some(depth) = server_at {
            if brace < depth {
                if !current.server_names.is_empty() || current.access_log.is_some() {
                    vhosts.push(current.clone());
                }
                server_at = None;
            }
        }
    }
    vhosts
}

fn empty_vhost() -> NginxVhost {
    NginxVhost {
        server_names: vec![],
        listen_ports: vec![],
        root: None,
        proxy_ports: vec![],
        access_log: None,
        error_log: None,
    }
}

fn capture_directive(current: &mut NginxVhost, trimmed: &str) {
    if trimmed.starts_with("server_name ") {
        let names: Vec<String> = trimmed["server_name ".len()..]
            .trim_end_matches(';')
            .split_whitespace()
            .filter(|n| *n != "_" && !n.is_empty())
            .map(|s| s.to_string())
            .collect();
        current.server_names.extend(names);
    } else if trimmed.starts_with("listen ") {
        let part = trimmed["listen ".len()..].trim_end_matches(';');
        let token = part.split_whitespace().next().unwrap_or(part);
        let port_str = token.rsplit(':').next().unwrap_or(token);
        if let Ok(p) = port_str.parse::<u16>() {
            current.listen_ports.push(p);
        }
    } else if trimmed.starts_with("root ") {
        current.root = Some(trimmed["root ".len()..].trim_end_matches(';').trim().to_string());
    } else if trimmed.contains("proxy_pass") {
        if let Some(port) = extract_url_port(trimmed) {
            current.proxy_ports.push(port);
        }
    } else if trimmed.starts_with("access_log ") {
        let path = trimmed["access_log ".len()..]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches(';');
        if !path.is_empty() && path != "off" {
            current.access_log = Some(path.to_string());
        }
    } else if trimmed.starts_with("error_log ") {
        let path = trimmed["error_log ".len()..]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches(';');
        if !path.is_empty() {
            current.error_log = Some(path.to_string());
        }
    }
}

pub fn vhost_log_sources(v: &NginxVhost, idx: usize) -> Vec<LogSourceRef> {
    let mut sources = Vec::new();
    let vhost_label = v
        .server_names
        .first()
        .cloned()
        .unwrap_or_else(|| format!("vhost-{}", idx));
    if let Some(path) = &v.access_log {
        sources.push(LogSourceRef {
            id: format!("nginx-access-{}", idx),
            kind: LogSourceKind::NginxAccess,
            path: Some(path.clone()),
            container_id: None,
            vhost: Some(vhost_label.clone()),
            readable: true,
            issue: None,
            fix_hint: None,
        });
    }
    if let Some(path) = &v.error_log {
        sources.push(LogSourceRef {
            id: format!("nginx-error-{}", idx),
            kind: LogSourceKind::NginxError,
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

fn extract_url_port(line: &str) -> Option<u16> {
    let after = line.split("proxy_pass").nth(1)?;
    let url = after.trim().trim_end_matches(';').trim();
    let host_port = url.split('/').nth(2).or(Some(url))?;
    let port_part = host_port.split(':').nth(1)?;
    port_part.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_many_vhosts() {
        let mut fixture = include_str!("../fixtures/nginx_t_sample.conf").to_string();
        for i in 0..20 {
            let port = 4100 + i;
            fixture.push_str(&format!(
                "\nserver {{\n    listen 80;\n    server_name app{i}.example.com;\n    access_log /var/log/nginx/app{i}.access.log;\n    error_log /var/log/nginx/app{i}.error.log;\n    location / {{\n        proxy_pass http://127.0.0.1:{port};\n    }}\n}}\n"
            ));
        }
        let vhosts = parse_nginx_t_vhosts(&fixture);
        assert!(vhosts.len() >= 20, "got {}", vhosts.len());
        let api = vhosts
            .iter()
            .find(|v| v.server_names.contains(&"api.example.com".to_string()));
        assert!(api.is_some());
        assert!(api.unwrap().proxy_ports.contains(&3001));
        let nested = vhosts
            .iter()
            .find(|v| v.server_names.contains(&"app19.example.com".to_string()))
            .unwrap();
        assert!(nested.proxy_ports.contains(&4119));
        assert!(nested.access_log.as_deref().unwrap().contains("app19"));
    }
}

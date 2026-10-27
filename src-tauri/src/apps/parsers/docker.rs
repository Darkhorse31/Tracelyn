use crate::apps::types::{AppKind, AppOrigin, AppStatus, LogSourceKind, LogSourceRef, TracelynApp};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct DockerPsLine {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Names")]
    names: String,
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "Status", default)]
    status_text: Option<String>,
    #[serde(rename = "Ports")]
    ports: Option<String>,
    #[serde(rename = "Labels", alias = "Label", default)]
    label: Option<String>,
}

pub fn parse_docker_ps_json_lines(server_id: &str, text: &str) -> Vec<TracelynApp> {
    let mut apps = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(row) = serde_json::from_str::<DockerPsLine>(line) {
            apps.push(docker_row_to_app(server_id, row));
        }
    }
    apps
}

fn docker_row_to_app(server_id: &str, row: DockerPsLine) -> TracelynApp {
    let name = row
        .names
        .split(',')
        .next()
        .unwrap_or(&row.names)
        .trim_start_matches('/')
        .to_string();
    let blob = format!(
        "{} {}",
        row.state,
        row.status_text.unwrap_or_default()
    )
    .to_lowercase();
    let oom = blob.contains("oom");
    let status = if blob.contains("running") && !oom {
        AppStatus::Online
    } else if oom || blob.contains("exited") || blob.contains("dead") {
        AppStatus::Errored
    } else {
        AppStatus::Stopped
    };
    let ports = parse_docker_ports(row.ports.as_deref().unwrap_or(""));
    let mut tags = vec!["docker".to_string()];
    if oom {
        tags.push("oom".into());
    }
    if let Some(labels) = &row.label {
        for part in labels.split(',') {
            if let Some(proj) = part.trim().strip_prefix("com.docker.compose.project=") {
                if !proj.is_empty() {
                    tags.push(format!("compose:{proj}"));
                }
            }
        }
    }
    TracelynApp {
        id: TracelynApp::discovered_id(server_id, AppKind::Docker, &name),
        server_id: server_id.to_string(),
        name,
        kind: AppKind::Docker,
        status,
        origin: AppOrigin::Discovered,
        hidden: false,
        pids: vec![],
        ports,
        domains: vec![],
        tags,
        log_sources: vec![LogSourceRef {
            id: format!("docker-{}", row.id),
            kind: LogSourceKind::Docker,
            path: None,
            container_id: Some(row.id),
            vhost: None,
            readable: true,
            issue: None,
            fix_hint: None,
        }],
        cpu_percent: None,
        memory_bytes: None,
        restarts: None,
        uptime_secs: None,
        cwd: None,
        interpreter: None,
        linked_app_ids: vec![],
        metrics: None,
    }
}

fn parse_docker_ports(ports: &str) -> Vec<u16> {
    let mut out = Vec::new();
    for part in ports.split(',') {
        for seg in part.split("->") {
            if let Some(host) = seg.split(':').nth(1) {
                if let Ok(p) = host.trim().parse::<u16>() {
                    out.push(p);
                }
            }
            if let Some(num) = seg.split('/').next() {
                if let Ok(p) = num.trim().parse::<u16>() {
                    out.push(p);
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_json_lines() {
        let fixture = include_str!("../fixtures/docker_ps.jsonl");
        let apps = parse_docker_ps_json_lines("s1", fixture);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].ports, vec![3000]);
    }
}

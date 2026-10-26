use crate::apps::types::{AppKind, AppOrigin, AppStatus, LogSourceKind, LogSourceRef, TracelynApp};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Pm2Process {
    name: String,
    pm_id: u32,
    pid: u32,
    #[serde(default)]
    monit: Pm2Monit,
    #[serde(default)]
    pm2_env: Pm2Env,
}

#[derive(Debug, Default, Deserialize)]
struct Pm2Monit {
    cpu: f64,
    memory: u64,
}

#[derive(Debug, Default, Deserialize)]
struct Pm2Env {
    status: Option<String>,
    restart_time: Option<u32>,
    pm_uptime: Option<u64>,
    exec_mode: Option<String>,
    instances: Option<u32>,
    pm_out_log_path: Option<String>,
    pm_err_log_path: Option<String>,
    pm_cwd: Option<String>,
    exec_interpreter: Option<String>,
}

pub fn parse_pm2_jlist(server_id: &str, json: &str) -> Vec<TracelynApp> {
    let processes: Vec<Pm2Process> = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    processes
        .into_iter()
        .map(|p| pm2_to_app(server_id, p))
        .collect()
}

fn pm2_tags(env: &Pm2Env) -> Vec<String> {
    let mut tags = Vec::new();
    if let Some(mode) = &env.exec_mode {
        if !mode.is_empty() {
            tags.push(mode.clone());
        }
    }
    if let Some(n) = env.instances {
        tags.push(format!("instances:{n}"));
    }
    tags
}

fn pm2_uptime_secs(pm_uptime: Option<u64>) -> Option<u64> {
    let ms = pm_uptime?;
    if ms > 1_000_000_000_000 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(ms);
        Some(now.saturating_sub(ms) / 1000)
    } else if ms > 10_000_000 {
        Some(ms / 1000)
    } else {
        Some(ms)
    }
}

fn pm2_to_app(server_id: &str, p: Pm2Process) -> TracelynApp {
    let name = p.name.clone();
    let status = match p.pm2_env.status.as_deref() {
        Some("online") => AppStatus::Online,
        Some("stopped") | Some("stopping") => AppStatus::Stopped,
        Some("errored") => AppStatus::Errored,
        _ => AppStatus::Unknown,
    };
    let mut log_sources = Vec::new();
    if let Some(path) = &p.pm2_env.pm_out_log_path {
        log_sources.push(LogSourceRef {
            id: format!("{}-stdout", p.pm_id),
            kind: LogSourceKind::Stdout,
            path: Some(path.clone()),
            container_id: None,
            vhost: None,
            readable: true,
            issue: None,
            fix_hint: None,
        });
    }
    if let Some(path) = &p.pm2_env.pm_err_log_path {
        log_sources.push(LogSourceRef {
            id: format!("{}-stderr", p.pm_id),
            kind: LogSourceKind::Stderr,
            path: Some(path.clone()),
            container_id: None,
            vhost: None,
            readable: true,
            issue: None,
            fix_hint: None,
        });
    }
    TracelynApp {
        id: TracelynApp::discovered_id(server_id, AppKind::Pm2, &name),
        server_id: server_id.to_string(),
        name,
        kind: AppKind::Pm2,
        status,
        origin: AppOrigin::Discovered,
        hidden: false,
        pids: if p.pid > 0 { vec![p.pid] } else { vec![] },
        ports: vec![],
        domains: vec![],
        tags: pm2_tags(&p.pm2_env),
        log_sources,
        cpu_percent: Some(p.monit.cpu),
        memory_bytes: Some(p.monit.memory),
        restarts: p.pm2_env.restart_time,
        uptime_secs: pm2_uptime_secs(p.pm2_env.pm_uptime),
        cwd: p.pm2_env.pm_cwd,
        interpreter: p.pm2_env.exec_interpreter,
        linked_app_ids: vec![],
        metrics: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_app_jlist() {
        let fixture = include_str!("../fixtures/pm2_jlist.json");
        let apps = parse_pm2_jlist("srv1", fixture);
        assert_eq!(apps.len(), 3);
        assert!(apps.iter().any(|a| a.name == "api-gateway"));
        let api = apps.iter().find(|a| a.name == "api-gateway").unwrap();
        assert_eq!(api.status, AppStatus::Online);
        assert_eq!(api.log_sources.len(), 2);
    }
}

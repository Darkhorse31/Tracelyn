use crate::apps::types::{AppKind, AppOrigin, AppStatus, LogSourceKind, LogSourceRef, TracelynApp};

/// Unit names that are distro infrastructure — excluded from app list.
pub const SYSTEMD_DENYLIST: &[&str] = &[
    "systemd-",
    "dbus",
    "network",
    "ssh",
    "cron",
    "rsyslog",
    "snapd",
    "ufw",
    "getty",
    "user@",
    "session-",
];

pub fn parse_systemctl_list(server_id: &str, text: &str) -> Vec<TracelynApp> {
    let mut apps = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("UNIT ") {
            continue;
        }
        let unit = line.split_whitespace().next().unwrap_or("");
        if !unit.ends_with(".service") {
            continue;
        }
        if SYSTEMD_DENYLIST.iter().any(|d| unit.starts_with(d)) {
            continue;
        }
        let name = unit.trim_end_matches(".service").to_string();
        let status = if line.contains(" running ") {
            AppStatus::Online
        } else if line.contains(" failed ") {
            AppStatus::Errored
        } else {
            AppStatus::Stopped
        };
        apps.push(TracelynApp {
            id: TracelynApp::discovered_id(server_id, AppKind::Systemd, &name),
            server_id: server_id.to_string(),
            name,
            kind: AppKind::Systemd,
            status,
            origin: AppOrigin::Discovered,
            hidden: false,
            pids: vec![],
            ports: vec![],
            domains: vec![],
            tags: vec!["systemd".to_string()],
            log_sources: vec![LogSourceRef {
                id: format!("journal-{}", unit),
                kind: LogSourceKind::Journald,
                path: Some(unit.to_string()),
                container_id: None,
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
        });
    }
    apps
}

/// Keep units that have a unit file under `/etc/systemd/system` (custom services).
pub fn filter_to_custom_unit_files(apps: Vec<TracelynApp>, ls_output: &str) -> Vec<TracelynApp> {
    let names: std::collections::HashSet<String> = ls_output
        .lines()
        .filter_map(|line| {
            let base = line.trim().rsplit('/').next()?;
            let name = base.trim_end_matches(".service");
            if name.is_empty() || name == base {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect();
    if names.is_empty() {
        return apps;
    }
    apps.into_iter().filter(|a| names.contains(&a.name)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_distro_units_and_keeps_custom_files() {
        let text = "\
nginx.service loaded active running A high performance web server
ssh.service loaded active running OpenSSH
cron.service loaded active running cron
my-api.service loaded active running My API
";
        let apps = parse_systemctl_list("s", text);
        assert!(apps.iter().any(|a| a.name == "nginx"));
        assert!(apps.iter().any(|a| a.name == "my-api"));
        assert!(apps.iter().all(|a| a.name != "ssh" && a.name != "cron"));
        let custom = filter_to_custom_unit_files(
            apps,
            "/etc/systemd/system/my-api.service\n",
        );
        assert_eq!(custom.len(), 1);
        assert_eq!(custom[0].name, "my-api");
        assert_eq!(custom[0].log_sources[0].kind, LogSourceKind::Journald);
    }
}

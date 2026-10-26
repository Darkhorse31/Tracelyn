use crate::apps::parsers::nginx_vhost::{parse_nginx_t_vhosts, vhost_log_sources};
use crate::apps::parsers::ss_listen::{parse_ss_ltnp, port_to_pid_map};
use crate::apps::types::{AppKind, AppOrigin, AppStatus, TracelynApp};
use std::collections::HashSet;

const SERVICE_NAMES: &[&str] = &[
    "mysql",
    "mariadb",
    "postgres",
    "mongod",
    "redis-server",
    "rabbitmq",
    "memcached",
];

pub fn attach_vhosts_and_ports(
    apps: &mut Vec<TracelynApp>,
    nginx_config: Option<&str>,
    ss_output: Option<&str>,
) {
    let vhosts = nginx_config.map(parse_nginx_t_vhosts).unwrap_or_default();
    let listeners = ss_output.map(parse_ss_ltnp).unwrap_or_default();
    let port_pid = port_to_pid_map(&listeners);

    for (idx, vh) in vhosts.iter().enumerate() {
        let sources = vhost_log_sources(vh, idx);
        let target_ports: HashSet<u16> = vh
            .proxy_ports
            .iter()
            .chain(vh.listen_ports.iter())
            .copied()
            .collect();

        let mut linked = false;
        for app in apps.iter_mut() {
            if app.pids.is_empty() {
                continue;
            }
            let pid = app.pids[0];
            for port in &target_ports {
                if port_pid.get(port) == Some(&pid) {
                    app.domains.extend(vh.server_names.clone());
                    app.ports.extend(target_ports.iter().copied());
                    app.log_sources.extend(sources.clone());
                    linked = true;
                    break;
                }
            }
            if linked {
                break;
            }
        }
        if !linked && !vh.server_names.is_empty() {
            let name = vh.server_names[0].clone();
            apps.push(TracelynApp {
                id: TracelynApp::discovered_id("", AppKind::Nginx, &name),
                server_id: String::new(),
                name: format!("nginx:{}", name),
                kind: AppKind::Nginx,
                status: AppStatus::Online,
                origin: AppOrigin::Discovered,
                hidden: false,
                pids: vec![],
                ports: vh.listen_ports.clone(),
                domains: vh.server_names.clone(),
                tags: vec!["nginx".to_string()],
                log_sources: sources,
                cpu_percent: None,
                memory_bytes: None,
                restarts: None,
                uptime_secs: None,
                cwd: vh.root.clone(),
                interpreter: None,
                linked_app_ids: vec![],
                metrics: None,
            });
        }
    }

    for app in apps.iter_mut() {
        app.domains.sort();
        app.domains.dedup();
        app.ports.sort_unstable();
        app.ports.dedup();
        if !app.server_id.is_empty() {
            app.id = TracelynApp::discovered_id(&app.server_id, app.kind, &app.name);
        }
    }

    let known_ports: HashSet<u16> = apps.iter().flat_map(|a| a.ports.iter().copied()).collect();
    for listener in &listeners {
        if known_ports.contains(&listener.port) {
            continue;
        }
        let lower = listener.process_name.to_lowercase();
        if !SERVICE_NAMES.iter().any(|s| lower.contains(s)) && listener.port > 1024 {
            continue;
        }
        let name = format!("{}:{}", listener.process_name, listener.port);
        if let Some(existing) = apps
            .iter_mut()
            .find(|app| app.kind == AppKind::Service && app.name == name)
        {
            for addr in &listener.addresses {
                if !existing.domains.contains(addr) {
                    existing.domains.push(addr.clone());
                }
            }
            if let Some(pid) = listener.pid {
                if !existing.pids.contains(&pid) {
                    existing.pids.push(pid);
                }
            }
            continue;
        }
        apps.push(TracelynApp {
            id: TracelynApp::discovered_id("", AppKind::Service, &name),
            server_id: String::new(),
            name,
            kind: AppKind::Service,
            status: AppStatus::Online,
            origin: AppOrigin::Discovered,
            hidden: false,
            pids: listener.pid.into_iter().collect(),
            ports: vec![listener.port],
            domains: listener.addresses.clone(),
            tags: vec!["service".to_string()],
            log_sources: vec![],
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

    for app in apps.iter_mut() {
        if app.server_id.is_empty() {
            continue;
        }
        if app.id.starts_with("app-") && app.id.len() < 20 {
            app.id = TracelynApp::discovered_id(&app.server_id, app.kind, &app.name);
        }
    }
}

pub fn apply_manual_overrides(apps: &mut Vec<TracelynApp>, overrides: &[TracelynApp]) {
    for manual in overrides {
        if let Some(existing) = apps.iter_mut().find(|a| a.id == manual.id) {
            if manual.hidden {
                existing.hidden = true;
            }
            if !manual.log_sources.is_empty() {
                existing.log_sources = manual.log_sources.clone();
            }
            if !manual.domains.is_empty() {
                existing.domains = manual.domains.clone();
            }
            if !manual.tags.is_empty() {
                existing.tags = manual.tags.clone();
            }
            if manual.origin == AppOrigin::Manual && manual.name != manual.id {
                existing.name = manual.name.clone();
                existing.kind = manual.kind;
                if !manual.ports.is_empty() {
                    existing.ports = manual.ports.clone();
                }
            }
        } else if manual.origin == AppOrigin::Manual && !manual.hidden {
            apps.push(manual.clone());
        }
    }
    apps.retain(|a| !a.hidden);
}

/// Service rows that share a process name and port (IPv4 and IPv6) become one app.
pub fn collapse_duplicate_services(apps: &mut Vec<TracelynApp>) {
    let mut out: Vec<TracelynApp> = Vec::new();
    for app in apps.drain(..) {
        let same = if app.kind == AppKind::Service {
            out.iter_mut().find(|prev| {
                prev.kind == AppKind::Service
                    && prev.name == app.name
                    && prev.ports == app.ports
            })
        } else {
            out.iter_mut().find(|prev| prev.id == app.id && !app.id.is_empty())
        };
        if let Some(prev) = same {
            for domain in app.domains {
                if !prev.domains.contains(&domain) {
                    prev.domains.push(domain);
                }
            }
            for pid in app.pids {
                if !prev.pids.contains(&pid) {
                    prev.pids.push(pid);
                }
            }
            for port in app.ports {
                if !prev.ports.contains(&port) {
                    prev.ports.push(port);
                }
            }
        } else {
            out.push(app);
        }
    }
    *apps = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_listeners_collapse_to_one_name() {
        let ss = "\
LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=1,fd=3))
LISTEN 0 128 [::]:22 [::]:* users:((\"sshd\",pid=1,fd=4))
LISTEN 0 128 0.0.0.0:5432 0.0.0.0:* users:((\"postgres\",pid=2,fd=5))
LISTEN 0 128 [::]:5432 [::]:* users:((\"postgres\",pid=2,fd=6))
";
        let mut apps = Vec::new();
        attach_vhosts_and_ports(&mut apps, None, Some(ss));
        let names: Vec<_> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["sshd:22", "postgres:5432"]);
        assert_eq!(apps[0].domains.len(), 2);
        assert!(apps[1].domains.iter().any(|d| d.contains("5432")));
    }
}

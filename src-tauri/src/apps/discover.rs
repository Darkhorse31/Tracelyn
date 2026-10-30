use crate::apps::merge::{apply_manual_overrides, attach_vhosts_and_ports, collapse_duplicate_services};
use crate::apps::parsers::apache_vhost::{
    apache_log_sources, apply_apache_custom_logs, parse_apache_s_and_vhosts,
};
use crate::apps::parsers::docker::parse_docker_ps_json_lines;
use crate::apps::parsers::pm2::parse_pm2_jlist;
use crate::apps::parsers::systemd::{filter_to_custom_unit_files, parse_systemctl_list};
use crate::apps::types::{AppKind, AppOrigin, AppStatus, TracelynApp};
use crate::ssh::{run_command, SshSession};
use std::collections::HashMap;

const PM2_HOME_PROBE: &str =
    "ls -d /root/.pm2 /home/*/.pm2 2>/dev/null | head -20";

pub async fn discover_apps(
    session: &SshSession,
    server_id: &str,
    manual: &[TracelynApp],
    sudo_password: Option<&str>,
) -> Result<Vec<TracelynApp>, String> {
    let mut apps = Vec::new();

    let pm2_json = run_probe(session, "pm2 jlist 2>/dev/null || echo '[]'", sudo_password).await?;
    apps.extend(parse_pm2_jlist(server_id, &pm2_json));

    if let Ok(homes) = run_probe(session, PM2_HOME_PROBE, sudo_password).await {
        for home in homes.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
            let cmd = format!(
                "PM2_HOME={} sudo -n -E pm2 jlist 2>/dev/null || PM2_HOME={} pm2 jlist 2>/dev/null",
                home,
                home
            );
            if let Ok(extra) = run_probe(session, &cmd, sudo_password).await {
                for app in parse_pm2_jlist(server_id, &extra) {
                    if !apps.iter().any(|a| a.name == app.name && a.kind == app.kind) {
                        apps.push(app);
                    }
                }
            }
        }
    }

    if let Ok(systemd) = run_probe(
        session,
        "systemctl list-units --type=service --state=running --no-pager --no-legend 2>/dev/null",
        sudo_password,
    )
    .await
    {
        let mut units = parse_systemctl_list(server_id, &systemd);
        if let Ok(files) = run_probe(
            session,
            "ls -1 /etc/systemd/system/*.service /etc/systemd/system/*/*.service 2>/dev/null",
            sudo_password,
        )
        .await
        {
            units = filter_to_custom_unit_files(units, &files);
        }
        apps.extend(units);
    }

    if let Ok(docker) = run_probe(
        session,
        "docker ps -a --filter status=running --filter status=exited --filter status=dead --format '{{json .}}' 2>/dev/null",
        sudo_password,
    )
    .await
    {
        apps.extend(parse_docker_ps_json_lines(server_id, &docker));
    }

    let nginx_t = run_probe(session, "nginx -T 2>/dev/null", sudo_password)
        .await
        .ok();
    let ss_out = run_probe(session, "ss -ltnpH 2>/dev/null || ss -ltn", sudo_password)
        .await
        .ok();

    attach_vhosts_and_ports(&mut apps, nginx_t.as_deref(), ss_out.as_deref());

    for app in &mut apps {
        if app.server_id.is_empty() {
            app.server_id = server_id.to_string();
            app.id = TracelynApp::discovered_id(server_id, app.kind, &app.name);
        }
    }

    if let Ok(apache) = run_probe(
        session,
        "apache2ctl -S 2>/dev/null || httpd -S 2>/dev/null",
        sudo_password,
    )
    .await
    {
        let mut vhosts = parse_apache_s_and_vhosts(&apache);
        if let Ok(grep) = run_probe(
            session,
            "grep -nE 'CustomLog|ErrorLog' /etc/apache2/sites-enabled/* /etc/httpd/conf.d/* /etc/httpd/conf/httpd.conf 2>/dev/null",
            sudo_password,
        )
        .await
        {
            apply_apache_custom_logs(&mut vhosts, &grep);
        }
        for (idx, vh) in vhosts.iter().enumerate() {
            let name = vh
                .server_names
                .first()
                .cloned()
                .unwrap_or_else(|| format!("apache-{idx}"));
            let id_name = format!("apache:{name}");
            apps.push(TracelynApp {
                id: TracelynApp::discovered_id(server_id, AppKind::Apache, &id_name),
                server_id: server_id.to_string(),
                name: id_name,
                kind: AppKind::Apache,
                status: AppStatus::Online,
                origin: AppOrigin::Discovered,
                hidden: false,
                pids: vec![],
                ports: vh.port.into_iter().collect(),
                domains: vh.server_names.clone(),
                tags: vec!["apache".into()],
                log_sources: apache_log_sources(vh, idx),
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
    }

    if let Ok(host) = run_probe(session, HOST_LOGS, sudo_password).await {
        let sources = parse_host_logs(&host);
        if !sources.is_empty() {
            apps.push(TracelynApp {
                id: TracelynApp::discovered_id(server_id, AppKind::Service, "Server logs"),
                server_id: server_id.to_string(),
                name: "Server logs".into(),
                kind: AppKind::Service,
                status: AppStatus::Online,
                origin: AppOrigin::Discovered,
                hidden: false,
                pids: vec![],
                ports: vec![],
                domains: vec![],
                tags: vec!["logs".into()],
                log_sources: sources,
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
    }

    attach_git_heads(session, &mut apps).await;
    apply_manual_overrides(&mut apps, manual);
    collapse_duplicate_services(&mut apps);
    apps.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(apps)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

async fn attach_git_heads(session: &SshSession, apps: &mut [TracelynApp]) {
    let mut cwds = Vec::new();
    for app in apps.iter() {
        if let Some(cwd) = app.cwd.as_deref().filter(|cwd| !cwd.is_empty()) {
            if !cwds.iter().any(|known: &String| known == cwd) {
                cwds.push(cwd.to_string());
            }
        }
    }
    cwds.truncate(20);
    if cwds.is_empty() {
        return;
    }
    let script = cwds
        .iter()
        .map(|cwd| {
            let quoted = shell_quote(cwd);
            format!("printf '%s\\t' {quoted}; git -C {quoted} rev-parse --short HEAD 2>/dev/null || printf '\\n'")
        })
        .collect::<Vec<_>>()
        .join(";");
    let Ok(out) = run_remote(session, &script).await else {
        return;
    };
    let mut heads = HashMap::new();
    for line in out.lines() {
        let Some((cwd, rev)) = line.split_once('\t') else {
            continue;
        };
        let rev = rev.trim();
        if rev.len() >= 7 && rev.chars().all(|c| c.is_ascii_hexdigit()) {
            heads.insert(cwd.to_string(), rev.to_string());
        }
    }
    for app in apps.iter_mut() {
        if let Some(cwd) = &app.cwd {
            if let Some(rev) = heads.get(cwd) {
                app.tags.push(format!("git:{rev}"));
            }
        }
    }
}

const HOST_LOGS: &str = r#"for p in /var/log/syslog /var/log/messages /var/log/nginx/error.log /var/log/apache2/error.log /var/log/httpd/error_log /var/log/otel/telemetry.json; do
  parent=$(dirname -- "$p")
  if [ -e "$p" ] || { [ -d "$parent" ] && [ ! -x "$parent" ]; }; then printf 'HOSTLOG %s\n' "$p"; fi
done
"#;

pub fn parse_host_logs(text: &str) -> Vec<crate::apps::types::LogSourceRef> {
    use crate::apps::types::{LogSourceKind, LogSourceRef};
    let mut sources = Vec::new();
    for line in text.lines() {
        let Some(path) = line.trim().strip_prefix("HOSTLOG ") else {
            continue;
        };
        let path = path.trim();
        if crate::insight::detect::validate_log_path(path).is_err() {
            continue;
        }
        let kind = if path.contains("telemetry.json") {
            LogSourceKind::Custom
        } else if path.contains("nginx") && path.contains("error") {
            LogSourceKind::NginxError
        } else {
            LogSourceKind::Custom
        };
        sources.push(LogSourceRef {
            id: format!("host-{}", sources.len()),
            kind,
            path: Some(path.to_string()),
            container_id: None,
            vhost: None,
            readable: true,
            issue: None,
            fix_hint: None,
        });
    }
    sources
}

async fn run_probe(session: &SshSession, cmd: &str, sudo_password: Option<&str>) -> Result<String, String> {
    let (wrapped, stdin) = if sudo_password.is_some() {
        (
            crate::setup::sudo_stdin_read(cmd),
            sudo_password.map(crate::setup::password_bytes),
        )
    } else {
        (crate::setup::privileged_read(cmd), None)
    };
    crate::ssh::run_command_captured(session, &wrapped, stdin)
        .await
        .map(|out| out.stdout)
        .map_err(|e| e.to_string())
}

async fn run_remote(session: &SshSession, cmd: &str) -> Result<String, String> {
    run_command(session, cmd).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod host_log_tests {
    use super::*;

    #[test]
    fn host_logs_include_syslog_and_otel() {
        let sources = parse_host_logs("HOSTLOG /var/log/syslog\nHOSTLOG /var/log/otel/telemetry.json\nHOSTLOG /tmp/a'b\n");
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[1].path.as_deref(), Some("/var/log/otel/telemetry.json"));
    }
}

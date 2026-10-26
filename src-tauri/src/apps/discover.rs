use crate::apps::merge::{apply_manual_overrides, attach_vhosts_and_ports, collapse_duplicate_services};
use crate::apps::parsers::apache_vhost::{
    apache_log_sources, apply_apache_custom_logs, parse_apache_s_and_vhosts,
};
use crate::apps::parsers::docker::parse_docker_ps_json_lines;
use crate::apps::parsers::pm2::parse_pm2_jlist;
use crate::apps::parsers::systemd::{filter_to_custom_unit_files, parse_systemctl_list};
use crate::apps::types::{AppKind, AppOrigin, AppStatus, TracelynApp};
use crate::ssh::{run_command, SshSession};

const PM2_HOME_PROBE: &str =
    "ls -d /root/.pm2 /home/*/.pm2 2>/dev/null | head -20";

pub async fn discover_apps(
    session: &SshSession,
    server_id: &str,
    manual: &[TracelynApp],
) -> Result<Vec<TracelynApp>, String> {
    let mut apps = Vec::new();

    let pm2_json = run_remote(session, "pm2 jlist 2>/dev/null || echo '[]'").await?;
    apps.extend(parse_pm2_jlist(server_id, &pm2_json));

    if let Ok(homes) = run_remote(session, PM2_HOME_PROBE).await {
        for home in homes.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
            let cmd = format!(
                "PM2_HOME={} sudo -n -E pm2 jlist 2>/dev/null || PM2_HOME={} pm2 jlist 2>/dev/null",
                home,
                home
            );
            if let Ok(extra) = run_remote(session, &cmd).await {
                for app in parse_pm2_jlist(server_id, &extra) {
                    if !apps.iter().any(|a| a.name == app.name && a.kind == app.kind) {
                        apps.push(app);
                    }
                }
            }
        }
    }

    if let Ok(systemd) = run_remote(
        session,
        "systemctl list-units --type=service --state=running --no-pager --no-legend 2>/dev/null",
    )
    .await
    {
        let mut units = parse_systemctl_list(server_id, &systemd);
        if let Ok(files) = run_remote(
            session,
            "ls -1 /etc/systemd/system/*.service /etc/systemd/system/*/*.service 2>/dev/null",
        )
        .await
        {
            units = filter_to_custom_unit_files(units, &files);
        }
        apps.extend(units);
    }

    if let Ok(docker) = run_remote(
        session,
        "sudo -n docker ps --format '{{json .}}' 2>/dev/null || docker ps --format '{{json .}}' 2>/dev/null",
    )
    .await
    {
        apps.extend(parse_docker_ps_json_lines(server_id, &docker));
    }

    let nginx_t = run_remote(session, "sudo -n nginx -T 2>/dev/null || nginx -T 2>/dev/null")
        .await
        .ok();
    let ss_out = run_remote(
        session,
        "sudo -n ss -ltnpH 2>/dev/null || ss -ltnpH 2>/dev/null || ss -ltn",
    )
    .await
    .ok();

    attach_vhosts_and_ports(&mut apps, nginx_t.as_deref(), ss_out.as_deref());

    for app in &mut apps {
        if app.server_id.is_empty() {
            app.server_id = server_id.to_string();
            app.id = TracelynApp::discovered_id(server_id, app.kind, &app.name);
        }
    }

    if let Ok(apache) = run_remote(
        session,
        "sudo -n apache2ctl -S 2>/dev/null || apache2ctl -S 2>/dev/null || sudo -n httpd -S 2>/dev/null || httpd -S 2>/dev/null",
    )
    .await
    {
        let mut vhosts = parse_apache_s_and_vhosts(&apache);
        if let Ok(grep) = run_remote(
            session,
            "grep -nE 'CustomLog|ErrorLog' /etc/apache2/sites-enabled/* /etc/httpd/conf.d/* /etc/httpd/conf/httpd.conf 2>/dev/null",
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

    apply_manual_overrides(&mut apps, manual);
    collapse_duplicate_services(&mut apps);
    apps.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(apps)
}

async fn run_remote(session: &SshSession, cmd: &str) -> Result<String, String> {
    run_command(session, cmd).await.map_err(|e| e.to_string())
}

pub mod wizard;

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupCheck {
    pub id: String,
    pub title: String,
    pub status: String,
    pub explanation: String,
    pub command: Option<String>,
    pub details: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EngineFact {
    pub id: String,
    pub title: String,
    pub configured: bool,
    pub phase: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AskedPath {
    pub id: String,
    pub group: String,
    pub label: String,
    pub path: String,
}

#[derive(Debug, Clone, Default)]
pub struct LocalFacts {
    pub ssh_error: Option<String>,
    pub redis_configured: bool,
    pub engines: Vec<EngineFact>,
    pub paths: Vec<AskedPath>,
    pub otel_spans: u64,
    pub otel_detail: String,
    pub otel_last_write: String,
}

#[derive(Debug, Clone, Default)]
pub struct ProbeReport {
    pub uid: u32,
    pub groups: String,
    pub sudo_code: i32,
    pub sudo_text: String,
    pub nginx_present: bool,
    pub nginx_files: Vec<(String, String)>,
    pub apache_present: bool,
    pub apache_files: Vec<(String, String)>,
    pub journal: String,
    pub journal_detail: String,
    pub pm2: Vec<(String, String)>,
    pub pm2_logs: Vec<(String, String)>,
    pub docker: String,
    pub redis: String,
    pub paths: BTreeMap<String, String>,
    pub path_mtime: BTreeMap<String, String>,
    pub otel_collector: String,
}

pub fn safe_user(user: &str) -> String {
    if !user.is_empty()
        && user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        user.to_string()
    } else {
        "USER".into()
    }
}

pub fn safe_remote_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() < 300
        && !path.chars().any(|c| c.is_control() || c == '\'')
}

pub fn probe_script(paths: &[AskedPath]) -> String {
    let mut script = String::from(BASE_PROBE);
    for path in paths.iter().filter(|p| safe_remote_path(&p.path)).take(40) {
        let id = path.id.replace(|c: char| !c.is_ascii_alphanumeric(), "");
        let quoted = format!("'{}'", path.path);
        script.push_str(&format!(
            "mt=$(stat -c %Y {quoted} 2>/dev/null || echo 0); if [ -e {quoted} ]; then if [ -r {quoted} ]; then printf '__PATH__\\t{id}\\tok\\t%s\\n' \"$mt\"; else printf '__PATH__\\t{id}\\tfail\\t%s\\n' \"$mt\"; fi; else printf '__PATH__\\t{id}\\tmissing\\t0\\n'; fi\n"
        ));
    }
    script.push_str("exit 0\n");
    script
}

const BASE_PROBE: &str = r#"printf '__UID__\t%s\n' "$(id -u 2>/dev/null || echo 0)"
printf '__GROUPS__\t%s\n' "$(id -nG 2>/dev/null | tr '\n' ' ')"
sudo_out=$(sudo -n -l 2>&1)
sudo_code=$?
printf '__SUDO__\t%s\t%s\n' "$sudo_code" "$(printf '%s' "$sudo_out" | tr '\n' ' ' | head -c 500)"
if [ -d /var/log/nginx ] || command -v nginx >/dev/null 2>&1; then
  printf '__NGINX__\tpresent\n'
  find /var/log/nginx -maxdepth 2 -type f \( -name '*access*log*' -o -name '*error*log*' \) 2>/dev/null | head -n 20 | while read -r f; do
    if [ -r "$f" ]; then printf '__NGINXFILE__\tok\t%s\n' "$f"; else printf '__NGINXFILE__\tfail\t%s\n' "$f"; fi
  done
else
  printf '__NGINX__\tabsent\n'
fi
if [ -d /var/log/apache2 ] || [ -d /var/log/httpd ] || command -v apache2ctl >/dev/null 2>&1 || command -v httpd >/dev/null 2>&1; then
  printf '__APACHE__\tpresent\n'
  find /var/log/apache2 /var/log/httpd -maxdepth 2 -type f \( -name '*access*log*' -o -name '*error*log*' \) 2>/dev/null | head -n 20 | while read -r f; do
    if [ -r "$f" ]; then printf '__APACHEFILE__\tok\t%s\n' "$f"; else printf '__APACHEFILE__\tfail\t%s\n' "$f"; fi
  done
else
  printf '__APACHE__\tabsent\n'
fi
if journalctl -n 1 -q --no-pager -o cat >/dev/null 2>&1; then
  printf '__JOURNAL__\tok\n'
else
  err=$(journalctl -n 1 -q --no-pager -o cat 2>&1 | tr '\n' ' ' | head -c 180)
  printf '__JOURNAL__\tfail\t%s\n' "$err"
fi
for d in /root/.pm2 /home/*/.pm2; do
  [ -d "$d" ] || continue
  if [ -r "$d" ] && [ -x "$d" ]; then printf '__PM2__\tok\t%s\n' "$d"; else printf '__PM2__\tfail\t%s\n' "$d"; fi
  for f in "$d"/logs/*.log; do
    [ -f "$f" ] || continue
    if [ -r "$f" ]; then printf '__PM2LOG__\tok\t%s\n' "$f"; else printf '__PM2LOG__\tfail\t%s\n' "$f"; fi
  done
done
if [ -S /var/run/docker.sock ]; then
  if [ -r /var/run/docker.sock ] && [ -w /var/run/docker.sock ]; then printf '__DOCKER__\tok\n'; else printf '__DOCKER__\tdenied\n'; fi
elif command -v docker >/dev/null 2>&1; then
  printf '__DOCKER__\tnosock\n'
else
  printf '__DOCKER__\tabsent\n'
fi
if command -v redis-cli >/dev/null 2>&1; then
  out=$(redis-cli -h 127.0.0.1 --no-auth-warning ping 2>&1 | tr '\n' ' ' | head -c 180)
  printf '__REDIS__\t%s\n' "$out"
else
  printf '__REDIS__\tabsent\n'
fi
svc=missing
if systemctl is-active --quiet otelcol-contrib 2>/dev/null; then svc=active
elif systemctl is-active --quiet otelcol 2>/dev/null; then svc=active
elif ps -eo args 2>/dev/null | grep -q '[o]telcol'; then svc=active
elif [ -f /etc/otelcol-contrib/config.yaml ] || [ -f /etc/otelcol/config.yaml ]; then svc=inactive
fi
printf '__OTELCOL__\t%s\n' "$svc"
"#;

pub fn parse_probe(text: &str) -> ProbeReport {
    let mut report = ProbeReport {
        sudo_code: 1,
        ..ProbeReport::default()
    };
    for line in text.lines() {
        let mut parts = line.split('\t');
        let tag = parts.next().unwrap_or("");
        match tag {
            "__UID__" => report.uid = parts.next().unwrap_or("0").trim().parse().unwrap_or(0),
            "__GROUPS__" => report.groups = parts.next().unwrap_or("").trim().to_string(),
            "__SUDO__" => {
                report.sudo_code = parts.next().unwrap_or("1").trim().parse().unwrap_or(1);
                report.sudo_text = parts.next().unwrap_or("").trim().to_string();
            }
            "__NGINX__" => report.nginx_present = parts.next().unwrap_or("") == "present",
            "__NGINXFILE__" => push_file(&mut report.nginx_files, &mut parts),
            "__APACHE__" => report.apache_present = parts.next().unwrap_or("") == "present",
            "__APACHEFILE__" => push_file(&mut report.apache_files, &mut parts),
            "__JOURNAL__" => {
                report.journal = parts.next().unwrap_or("fail").to_string();
                report.journal_detail = parts.next().unwrap_or("").to_string();
            }
            "__PM2__" => push_file(&mut report.pm2, &mut parts),
            "__PM2LOG__" => push_file(&mut report.pm2_logs, &mut parts),
            "__DOCKER__" => report.docker = parts.next().unwrap_or("absent").to_string(),
            "__REDIS__" => report.redis = parts.next().unwrap_or("").trim().to_string(),
            "__PATH__" => {
                let id = parts.next().unwrap_or("").to_string();
                let status = parts.next().unwrap_or("fail").to_string();
                let extra = parts.next().unwrap_or("").to_string();
                if !id.is_empty() {
                    report.paths.insert(id.clone(), status);
                    if extra.chars().all(|c| c.is_ascii_digit()) && !extra.is_empty() {
                        report.path_mtime.insert(id, extra);
                    }
                }
            }
            "__OTELCOL__" => report.otel_collector = parts.next().unwrap_or("missing").trim().to_string(),
            _ => {}
        }
    }
    report
}

fn push_file(out: &mut Vec<(String, String)>, parts: &mut std::str::Split<char>) {
    let status = parts.next().unwrap_or("fail").to_string();
    let path = parts.next().unwrap_or("").to_string();
    out.push((status, path));
}

pub fn build_checks(username: &str, probe: Option<&ProbeReport>, local: &LocalFacts) -> Vec<SetupCheck> {
    let user = safe_user(username);
    let mut checks = vec![ssh_check(probe, local)];
    checks.push(web_log_check(
        "nginx-logs",
        "Nginx log access",
        &user,
        probe,
        probe.map(|p| p.nginx_present),
        probe.map(|p| p.nginx_files.as_slice()),
        "/var/log/nginx",
    ));
    checks.push(web_log_check(
        "apache-logs",
        "Apache log access",
        &user,
        probe,
        probe.map(|p| p.apache_present),
        probe.map(|p| p.apache_files.as_slice()),
        "/var/log/apache2 or /var/log/httpd",
    ));
    checks.push(journal_check(&user, probe));
    checks.push(pm2_check(&user, probe));
    checks.push(docker_check(&user, probe));
    checks.push(sudo_check(&user, probe));
    checks.push(redis_check(probe, local.redis_configured));
    for engine in &local.engines {
        checks.push(engine_check(engine));
    }
    checks.push(otel_check(&user, probe, local));
    checks.push(path_group_check(
        "app",
        "Application log paths",
        "No application log file is known yet. Connect the server and let discovery run, or add a path on the application.",
        &user,
        probe,
        local,
    ));
    checks
}

fn ssh_check(probe: Option<&ProbeReport>, local: &LocalFacts) -> SetupCheck {
    if probe.is_some() {
        return row(
            "ssh",
            "SSH session",
            "pass",
            "Opened a read-only SSH session and inspected permissions. Nothing on the server was changed.",
            None,
            vec![],
        );
    }
    if let Some(err) = &local.ssh_error {
        return row(
            "ssh",
            "SSH session",
            "fail",
            &format!("Tracelyn could not open SSH to run the checks: {err}"),
            None,
            vec![],
        );
    }
    row(
        "ssh",
        "SSH session",
        "pending",
        "Connect the server, then choose Re-check all. The commands below are ready to copy and are not run by Tracelyn.",
        None,
        vec![],
    )
}

fn web_log_check(
    id: &str,
    title: &str,
    user: &str,
    probe: Option<&ProbeReport>,
    present: Option<bool>,
    files: Option<&[(String, String)]>,
    where_: &str,
) -> SetupCheck {
    let Some(present) = present else {
        return row(
            id,
            title,
            "pending",
            &format!("This check reads {where_} over SSH. It does not change group membership."),
            Some(usermod(user, "adm")),
            vec!["After the command, disconnect and connect again so the new group applies.".into()],
        );
    };
    if !present {
        return row(
            id,
            title,
            "pass",
            &format!("No {title} files are installed, so this access is not required."),
            None,
            vec![],
        );
    }
    let files = files.unwrap_or(&[]);
    if files.is_empty() {
        return row(
            id,
            title,
            "warning",
            &format!("{title} looks installed, but no access or error log file was found under {where_}."),
            None,
            vec![],
        );
    }
    let failed: Vec<_> = files.iter().filter(|(s, _)| s != "ok").cloned().collect();
    let details = files
        .iter()
        .map(|(s, p)| format!("{p} — {}", if s == "ok" { "readable" } else { "not readable" }))
        .collect();
    if failed.is_empty() {
        return row(
            id,
            title,
            "pass",
            "The SSH user can read the log files Tracelyn tails.",
            None,
            details,
        );
    }
    let root = probe.map(|p| p.uid == 0).unwrap_or(false);
    let in_adm = probe.map(|p| in_group(&p.groups, "adm") || p.uid == 0).unwrap_or(false);
    let command = if !root && !in_adm {
        usermod(user, "adm")
    } else {
        let list = failed
            .iter()
            .take(3)
            .map(|(_, p)| p.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        format!("sudo chmod a+r {list}")
    };
    row(
        id,
        title,
        "fail",
        &format!(
            "Tracelyn cannot read one or more logs in {where_}. On Debian and Ubuntu the adm group is the usual fix. Disconnect and connect again after changing groups."
        ),
        Some(command),
        details,
    )
}

fn journal_check(user: &str, probe: Option<&ProbeReport>) -> SetupCheck {
    let Some(probe) = probe else {
        return row(
            "journald",
            "journald access",
            "pending",
            "Tracelyn reads service journals with journalctl. It does not write to the journal.",
            Some(usermod(user, "systemd-journal")),
            vec!["Disconnect and connect again after joining the group.".into()],
        );
    };
    if probe.journal == "ok" {
        return row(
            "journald",
            "journald access",
            "pass",
            "journalctl returned a line, so the SSH user can read the journal.",
            None,
            vec![],
        );
    }
    let denied = probe.journal_detail.to_lowercase().contains("permission")
        || probe.journal_detail.to_lowercase().contains("not permitted")
        || !in_group(&probe.groups, "systemd-journal") && probe.uid != 0;
    row(
        "journald",
        "journald access",
        "fail",
        &format!(
            "journalctl did not succeed{}. Add the SSH user to systemd-journal, then disconnect and connect again.",
            if probe.journal_detail.is_empty() {
                String::new()
            } else {
                format!(" ({})", probe.journal_detail)
            }
        ),
        Some(if denied {
            usermod(user, "systemd-journal")
        } else {
            format!("sudo usermod -aG systemd-journal {user}")
        }),
        vec![],
    )
}

fn pm2_check(user: &str, probe: Option<&ProbeReport>) -> SetupCheck {
    let Some(probe) = probe else {
        return row(
            "pm2",
            "PM2 home and logs",
            "pending",
            "Tracelyn looks in ~/.pm2 and ~/.pm2/logs for each user. It only lists and reads those files.",
            None,
            vec![],
        );
    };
    if probe.pm2.is_empty() && probe.pm2_logs.is_empty() {
        return row(
            "pm2",
            "PM2 home and logs",
            "pass",
            "No PM2 home was found. Nothing to read, which is fine when this server does not use PM2.",
            None,
            vec![],
        );
    }
    let mut details = Vec::new();
    let mut bad: Vec<String> = Vec::new();
    for (status, path) in probe.pm2.iter().chain(probe.pm2_logs.iter()) {
        details.push(format!(
            "{path} — {}",
            if status == "ok" { "readable" } else { "not readable" }
        ));
        if status != "ok" {
            bad.push(path.clone());
        }
    }
    if bad.is_empty() {
        return row(
            "pm2",
            "PM2 home and logs",
            "pass",
            "The SSH user can read the PM2 home directories and log files that were found.",
            None,
            details,
        );
    }
    let target = bad[0].clone();
    row(
        "pm2",
        "PM2 home and logs",
        "fail",
        &format!("PM2 data for another user is not readable by {user}. Grant read access, then re-check. Tracelyn does not change the files itself."),
        Some(format!("sudo chmod -R a+rX {target}")),
        details,
    )
}

fn docker_check(user: &str, probe: Option<&ProbeReport>) -> SetupCheck {
    let Some(probe) = probe else {
        return row(
            "docker",
            "Docker socket",
            "pending",
            "Tracelyn reads the Docker socket to list containers. It does not create or change containers during this check.",
            Some(usermod(user, "docker")),
            vec!["Disconnect and connect again after joining the docker group.".into()],
        );
    };
    match probe.docker.as_str() {
        "ok" => row(
            "docker",
            "Docker socket",
            "pass",
            "The SSH user can read and write the Docker socket, which is what the Docker API requires.",
            None,
            vec![],
        ),
        "absent" => row(
            "docker",
            "Docker socket",
            "pass",
            "Docker is not installed. Skip this if the server does not run containers.",
            None,
            vec![],
        ),
        "nosock" => row(
            "docker",
            "Docker socket",
            "warning",
            "The docker command exists, but /var/run/docker.sock was not found. Rootless Docker or a custom socket path will not show up here.",
            None,
            vec![],
        ),
        _ => row(
            "docker",
            "Docker socket",
            "fail",
            "The Docker socket exists, but this SSH user cannot use it. Joining the docker group allows container listing. Disconnect and connect again afterward.",
            Some(usermod(user, "docker")),
            vec![],
        ),
    }
}

fn sudo_check(user: &str, probe: Option<&ProbeReport>) -> SetupCheck {
    let command = format!(
        "printf '%s\\n' '{user} ALL=(ALL) NOPASSWD: /bin/systemctl, /usr/bin/systemctl, /usr/bin/docker, /usr/bin/nginx, /usr/sbin/nginx, /usr/sbin/apache2ctl, /usr/sbin/apachectl' | sudo tee /etc/sudoers.d/tracelyn"
    );
    let Some(probe) = probe else {
        return row(
            "sudo",
            "Operations sudo",
            "pending",
            "Operations mode restarts services and needs passwordless sudo, or an SSH user that can already run those commands. Review the command before you run it. This check only lists sudo rights.",
            Some(command),
            vec![],
        );
    };
    if probe.uid == 0 {
        return row(
            "sudo",
            "Operations sudo",
            "pass",
            "The SSH user is root, so Operations commands do not need a sudo password.",
            None,
            vec![],
        );
    }
    let text = probe.sudo_text.to_lowercase();
    if probe.sudo_code == 0 && (text.contains("nopasswd") || text.contains("(all)")) {
        return row(
            "sudo",
            "Operations sudo",
            "pass",
            "sudo -n -l succeeded. Passwordless sudo is available for Operations mode.",
            None,
            vec![probe.sudo_text.clone()],
        );
    }
    row(
        "sudo",
        "Operations sudo",
        "warning",
        "Operations mode is off until this user can run systemctl, docker, nginx, or Apache without a password prompt. The command writes a sudoers drop-in. Read it before you run it. Tracelyn did not change sudoers.",
        Some(command),
        vec![probe.sudo_text.clone()],
    )
}

fn redis_check(probe: Option<&ProbeReport>, configured: bool) -> SetupCheck {
    let Some(probe) = probe else {
        return row(
            "redis",
            "Redis credentials",
            "pending",
            if configured {
                "A Redis password or ACL user is saved in Windows Credential Manager. Re-check after connecting to see if Redis answers."
            } else {
                "Save the Redis password on the Redis page if the server requires one. This check uses redis-cli ping and does not print the password."
            },
            None,
            vec![],
        );
    };
    let ping = probe.redis.to_lowercase();
    if ping == "absent" && !configured {
        return row(
            "redis",
            "Redis credentials",
            "pass",
            "redis-cli is not installed and no Redis password is saved. Skip this if the server has no Redis.",
            None,
            vec![],
        );
    }
    if ping == "absent" {
        return row(
            "redis",
            "Redis credentials",
            "fail",
            "A Redis password is saved, but redis-cli was not found on the server, so Tracelyn cannot reach Redis.",
            None,
            vec![],
        );
    }
    if ping.contains("pong") {
        return row(
            "redis",
            "Redis credentials",
            "pass",
            "Redis answered PING on 127.0.0.1. Saved credentials are optional when the server allows this ping.",
            None,
            vec![probe.redis.clone()],
        );
    }
    if ping.contains("noauth") || ping.contains("wrongpass") {
        return row(
            "redis",
            "Redis credentials",
            if configured { "warning" } else { "fail" },
            if configured {
                "Redis refused an unauthenticated ping. That is expected when requirepass is set. The saved password stays in Credential Manager and is not sent in this check. Confirm the Redis page shows Live."
            } else {
                "Redis requires a password. Set it on the Redis page. Tracelyn stores it in Windows Credential Manager and does not put it in the setup command."
            },
            None,
            vec![probe.redis.clone()],
        );
    }
    row(
        "redis",
        "Redis credentials",
        "fail",
        "Redis did not answer PING. Check that it is running, and set the port, socket, or Docker container on the Redis page.",
        None,
        vec![probe.redis.clone()],
    )
}

fn engine_check(engine: &EngineFact) -> SetupCheck {
    let phase = engine.phase.as_deref().unwrap_or("");
    let (status, explanation) = if !engine.configured && (phase.is_empty() || phase == "not_installed") {
        (
            "pass",
            format!(
                "{} is not configured. Add it on the Infrastructure page only if this server runs it. Tracelyn does not create database users.",
                engine.title
            ),
        )
    } else if phase == "ok" || phase == "partial" {
        (
            "pass",
            format!(
                "{} answered the last read-only sample{}.",
                engine.title,
                if engine.configured { " and credentials are saved" } else { "" }
            ),
        )
    } else if phase == "auth_needed" {
        (
            "fail",
            format!(
                "{} rejected the saved credentials or needs a username and password. Set them on the Infrastructure page. They stay in Windows Credential Manager.",
                engine.title
            ),
        )
    } else if phase == "permission_denied" {
        (
            "fail",
            format!(
                "The {} role can connect but cannot read the monitoring views. Grant read-only monitoring privileges. Tracelyn does not change grants.",
                engine.title
            ),
        )
    } else if phase == "not_installed" {
        (
            "warning",
            format!(
                "Credentials for {} are saved, but the CLI or service was not found on the server.",
                engine.title
            ),
        )
    } else if engine.configured {
        (
            "warning",
            format!(
                "Credentials or a target for {} are saved, but there is no successful sample yet. Open Infrastructure after the server is connected.",
                engine.title
            ),
        )
    } else {
        (
            "warning",
            format!(
                "{} has not been checked yet. Open Infrastructure to run the read-only sample.",
                engine.title
            ),
        )
    };
    row(&engine.id, &format!("{} credentials", engine.title), status, &explanation, None, vec![])
}

fn otel_check(user: &str, probe: Option<&ProbeReport>, local: &LocalFacts) -> SetupCheck {
    let mut check = path_group_check(
        "otel",
        "OpenTelemetry file exporter",
        "No collector config was found. Tracelyn looked at otelcol-contrib, otelcol, and /var/log/otel.",
        user,
        probe,
        local,
    );
    let collector = probe.map(|item| item.otel_collector.as_str()).unwrap_or("");
    let mut details = Vec::new();
    if !collector.is_empty() {
        details.push(format!("Collector service: {collector}"));
    }
    details.push(format!("Spans stored: {}", local.otel_spans));
    if !local.otel_last_write.is_empty() {
        details.push(format!("Last write: {}", local.otel_last_write));
    }
    if !local.otel_detail.is_empty() {
        details.push(local.otel_detail.clone());
    }
    details.append(&mut check.details);
    check.details = details;
    if collector == "inactive" || collector == "missing" {
        if check.status == "pass" {
            check.status = "warning".into();
        }
        check.explanation = "The collector service is not active, so new spans are not being written. File access is listed below.".into();
    } else if check.status != "pass" && !local.otel_detail.is_empty() {
        check.explanation = local.otel_detail.clone();
    }
    check
}

fn path_group_check(
    group: &str,
    title: &str,
    empty: &str,
    user: &str,
    probe: Option<&ProbeReport>,
    local: &LocalFacts,
) -> SetupCheck {
    let id = if group == "otel" { "otel" } else { "app-logs" };
    let paths: Vec<_> = local.paths.iter().filter(|p| p.group == group).collect();
    if paths.is_empty() {
        return row(id, title, "warning", empty, None, vec![]);
    }
    if probe.is_none() {
        return row(
            id,
            title,
            "pending",
            "Connect the server to test whether each path is readable. Tracelyn only uses test -r.",
            None,
            paths.iter().map(|p| format!("{} — {}", p.label, p.path)).collect(),
        );
    }
    let probe = probe.unwrap();
    let mut details = Vec::new();
    let mut any_fail = false;
    for path in &paths {
        if !safe_remote_path(&path.path) {
            details.push(format!("{} — {} — skipped (unusual path)", path.label, path.path));
            any_fail = true;
            continue;
        }
        let status = probe.paths.get(&path.id).map(String::as_str).unwrap_or("missing");
        let word = match status {
            "ok" => "readable",
            "fail" => "not readable",
            _ => "missing",
        };
        if status != "ok" {
            any_fail = true;
        }
        details.push(format!("{} — {} — {word}", path.label, path.path));
    }
    if !any_fail {
        return row(id, title, "pass", "Every listed path is readable by the SSH user.", None, details);
    }
    row(
        id,
        title,
        "fail",
        "At least one path is missing or not readable. The command is for you to run. Tracelyn does not change permissions.",
        failed_path_command(&paths, probe, user),
        details,
    )
}

fn failed_path_command(paths: &[&AskedPath], probe: &ProbeReport, user: &str) -> Option<String> {
    let path = paths.iter().find(|p| {
        !safe_remote_path(&p.path) || probe.paths.get(&p.id).map(String::as_str) != Some("ok")
    })?;
    if path.path.contains("/var/log/nginx")
        || path.path.contains("/var/log/apache")
        || path.path.contains("/var/log/httpd")
    {
        return Some(usermod(user, "adm"));
    }
    if safe_remote_path(&path.path) {
        return Some(format!("sudo chmod a+r {}", path.path));
    }
    None
}

fn usermod(user: &str, group: &str) -> String {
    format!("sudo usermod -aG {group} {user}")
}

fn in_group(groups: &str, name: &str) -> bool {
    groups.split_whitespace().any(|g| g == name)
}

fn row(
    id: &str,
    title: &str,
    status: &str,
    explanation: &str,
    command: Option<String>,
    details: Vec<String>,
) -> SetupCheck {
    SetupCheck {
        id: id.into(),
        title: title.into(),
        status: status.into(),
        explanation: explanation.into(),
        command,
        details,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_script_never_modifies_the_server() {
        let script = probe_script(&[AskedPath {
            id: "p0".into(),
            group: "app".into(),
            label: "api".into(),
            path: "/var/log/api.log".into(),
        }]);
        let lower = script.to_lowercase();
        for forbidden in ["usermod", "chmod", "chown", " tee ", " rm ", " mv ", "systemctl restart", "apt ", "dnf "] {
            assert!(!lower.contains(forbidden), "{forbidden} in probe");
        }
        assert!(script.contains("sudo -n -l"));
        assert!(script.contains("exit 0"));
        assert!(script.contains("/var/log/api.log"));
    }

    #[test]
    fn classifies_permissions_and_fills_the_username() {
        let text = "\
__UID__\t1000
__GROUPS__\tdeploy
__SUDO__\t1\ta password is required
__NGINX__\tpresent
__NGINXFILE__\tfail\t/var/log/nginx/access.log
__APACHE__\tabsent
__JOURNAL__\tfail\tPermission denied
__DOCKER__\tdenied
__REDIS__\tNOAUTH Authentication required
";
        let probe = parse_probe(text);
        assert_eq!(probe.uid, 1000);
        assert!(probe.nginx_present);
        let local = LocalFacts {
            redis_configured: false,
            engines: vec![EngineFact {
                id: "postgres".into(),
                title: "PostgreSQL".into(),
                configured: true,
                phase: Some("auth_needed".into()),
            }],
            paths: vec![AskedPath {
                id: "p0".into(),
                group: "app".into(),
                label: "api".into(),
                path: "/var/log/api/app.log".into(),
            }],
            ..LocalFacts::default()
        };
        let checks = build_checks("deploy", Some(&probe), &local);
        let nginx = checks.iter().find(|c| c.id == "nginx-logs").unwrap();
        assert_eq!(nginx.status, "fail");
        assert_eq!(nginx.command.as_deref(), Some("sudo usermod -aG adm deploy"));
        assert_eq!(checks.iter().find(|c| c.id == "apache-logs").unwrap().status, "pass");
        assert_eq!(checks.iter().find(|c| c.id == "journald").unwrap().status, "fail");
        assert!(checks.iter().find(|c| c.id == "journald").unwrap().command.as_deref().unwrap().contains("deploy"));
        assert_eq!(checks.iter().find(|c| c.id == "docker").unwrap().status, "fail");
        assert_eq!(checks.iter().find(|c| c.id == "sudo").unwrap().status, "warning");
        assert_eq!(checks.iter().find(|c| c.id == "redis").unwrap().status, "fail");
        assert_eq!(checks.iter().find(|c| c.id == "postgres").unwrap().status, "fail");
        assert_eq!(checks.iter().find(|c| c.id == "pm2").unwrap().status, "pass");
    }

    #[test]
    fn readable_nginx_and_otel_pass() {
        let text = "\
__UID__\t0
__GROUPS__\troot
__SUDO__\t0\tUser root may run the following commands
__NGINX__\tpresent
__NGINXFILE__\tok\t/var/log/nginx/access.log
__APACHE__\tabsent
__JOURNAL__\tok
__PM2__\tok\t/root/.pm2
__PM2LOG__\tok\t/root/.pm2/logs/api-out.log
__DOCKER__\tok
__REDIS__\tPONG
__PATH__\tp0\tok
";
        let probe = parse_probe(text);
        let local = LocalFacts {
            redis_configured: true,
            paths: vec![AskedPath {
                id: "p0".into(),
                group: "otel".into(),
                label: "api".into(),
                path: "/var/log/otel/api.json".into(),
            }],
            ..LocalFacts::default()
        };
        let checks = build_checks("root", Some(&probe), &local);
        for id in ["nginx-logs", "journald", "pm2", "docker", "sudo", "redis", "otel"] {
            assert_eq!(checks.iter().find(|c| c.id == id).unwrap().status, "pass", "{id}");
        }
    }
}

pub fn shell_single(script: &str) -> String {
    format!("'{}'", script.replace('\'', "'\"'\"'"))
}

/// Elevate read-only probes when noninteractive sudo is available.
pub fn privileged_read(script: &str) -> String {
    let quoted = shell_single(script);
    format!("if [ \"$(id -u)\" = 0 ]; then sh -c {quoted}; elif sudo -n true 2>/dev/null; then sudo -n sh -c {quoted}; else sh -c {quoted}; fi")
}

/// Run a script with `sudo -S`. The password is sent on stdin and is never placed in the command.
pub fn sudo_stdin_read(script: &str) -> String {
    let quoted = shell_single(script);
    format!("if [ \"$(id -u)\" = 0 ]; then sh -c {quoted}; else sudo -S -p '' -- sh -c {quoted}; fi")
}

pub fn password_bytes(password: &str) -> Vec<u8> {
    let mut bytes = password.as_bytes().to_vec();
    if !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    bytes
}

#[cfg(test)]
mod privileged_read_tests {
    #[test]
    fn quotes_script() {
        let script = super::privileged_read("printf '%s' \"$HOME\"");
        assert!(script.contains("sudo -n sh -c"));
        assert!(script.contains("'\"'\"'"));
        assert!(!script.contains("sudo -S"));
    }

    #[test]
    fn sudo_password_stays_off_the_command_line() {
        let script = super::sudo_stdin_read("printf '%s' \"$HOME\"");
        assert!(script.contains("sudo -S -p '' -- sh -c"));
        assert!(!script.contains("hunter2"));
        let bytes = super::password_bytes("hunter2");
        assert_eq!(bytes, b"hunter2\n");
    }
}

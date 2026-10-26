use super::{capacity, cron, model::*, nginx, parsers, presence, scripts, store, InfraState};
use crate::{metrics_ingest::now_ms, ssh::SshSession, state::AppState};
use russh::ChannelMsg;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::sync::{Mutex, Semaphore};

const LIMIT: usize = 524_288;
struct Output {
    text: String,
    error: String,
    code: u32,
    truncated: bool,
}

async fn execute(session: &SshSession, slots: &Semaphore, command: &str) -> Result<Output, String> {
    let _slot = slots.acquire().await.map_err(|_| "Collector stopped")?;
    let future = async {
        let mut channel = session
            .channel_open_session()
            .await
            .map_err(|_| "Could not open an SSH channel")?;
        let bounded=format!("if command -v timeout >/dev/null 2>&1; then timeout -k 1s 10s sh -c {}; else sh -c {}; fi",scripts::quote(command),scripts::quote(command));
        channel
            .exec(true, bounded)
            .await
            .map_err(|_| "Could not execute the read-only collector")?;
        let result = tokio::time::timeout(Duration::from_secs(12), async {
            let mut out = Output {
                text: String::new(),
                error: String::new(),
                code: 0,
                truncated: false,
            };
            while let Some(message) = channel.wait().await {
                match message {
                    ChannelMsg::Data { data } => {
                        if out.text.len() + out.error.len() + data.len() > LIMIT {
                            out.truncated = true;
                            break;
                        }
                        out.text.push_str(&String::from_utf8_lossy(&data));
                    }
                    ChannelMsg::ExtendedData { data, .. } => {
                        if out.text.len() + out.error.len() + data.len() > LIMIT {
                            out.truncated = true;
                            break;
                        }
                        out.error.push_str(&String::from_utf8_lossy(&data));
                    }
                    ChannelMsg::ExitStatus { exit_status } => {
                        out.code = exit_status;
                        break;
                    }
                    ChannelMsg::Eof => {}
                    _ => {}
                }
            }
            out
        })
        .await;
        let _ = tokio::time::timeout(Duration::from_secs(1), channel.close()).await;
        result.map_err(|_| "Read-only collector timed out after 12 seconds".to_string())
    };
    tokio::time::timeout(Duration::from_secs(15), future)
        .await
        .map_err(|_| "SSH channel timed out after 15 seconds".to_string())?
}

fn failure(name: &str, raw: &str, code: u32) -> Snapshot {
    let lower = raw.to_ascii_lowercase();
    let phase = if code == 124 || lower.contains("timed out") || lower.contains("timeout") {
        Phase::TimedOut
    } else if lower.contains("permission denied")
        || lower.contains("not permitted")
        || lower.contains("forbidden")
        || lower.contains("insufficient privilege")
        || lower.contains("insufficient permissions")
        || lower.contains("not seeing messages")
    {
        Phase::PermissionDenied
    } else if lower.contains("access denied")
        || lower.contains("authentication")
        || lower.contains("unauthorized")
        || lower.contains("401")
        || lower.contains("no password")
        || lower.contains("not authorized")
        || lower.contains("no pg_hba")
    {
        Phase::AuthNeeded
    } else if code == 127
        || lower.contains("not found")
        || lower.contains("not installed")
        || lower.contains("no such file")
    {
        Phase::NotInstalled
    } else {
        Phase::Error
    };
    let message = match phase {
        Phase::AuthNeeded => "Authentication is required or the saved credentials were rejected.",
        Phase::PermissionDenied => "The SSH user does not have read permission.",
        Phase::NotInstalled => "The read-only CLI, socket, or service was not found.",
        Phase::TimedOut => "The collector exceeded its time limit.",
        Phase::InstalledStopped => "The service is installed but not running.",
        Phase::NotConfigured => "The service is installed and has nothing configured for this check.",
        _ => "The collector failed.",
    };
    let mut snapshot = Snapshot::problem(name, phase, message, match name {
        "mysql"|"postgres"|"mongo"=>"Set a local database target and optional credentials here. Grant monitoring/read privileges. Verify the CLI or Docker container is available.",
        "rabbitmq"=>"Allow rabbitmqctl read access, or configure credentials for a management API bound to localhost:15672.",
        "kafka"=>"Make kafka-topics and kafka-consumer-groups available for localhost:9092. Secured clusters need an existing CLI client configuration.",
        "haproxy"=>"Grant read access to the HAProxy stats socket (socat/nc), or the CSV stats endpoint on localhost:8404/stats.",
        "traefik"=>"Check the read API on localhost:8080. Keep it bound to localhost.",
        "caddy"=>"Check Caddy's admin API on localhost:2019.",
        "docker_infra"=>"Grant the SSH user access to the Docker socket and reconnect.",
        "kubernetes"=>"Check kubeconfig read permissions and RBAC access to nodes and pods for kubectl/k3s.",
        "updates"=>"Package lists are read from the existing cache. Ask your administrator to refresh stale apt/dnf metadata.",
        "fail2ban"=>"Grant read access to the fail2ban control socket. No jail changes are made.",
        "cron"=>"Read access to other users' crontabs and the system journal may require an administrator.",
        "certificates"=>"Grant access to the certificate files and install openssl if needed. Private keys are never read.",
        "ssh_security"=>"Grant journal/auth.log read access, such as membership in systemd-journal or adm, and reconnect.",
        _=>"Check that the service is running and the SSH user can read its status.",
    });
    if phase == Phase::Error {
        let detail = raw.trim();
        if !detail.is_empty() {
            snapshot.detail = Some(detail.chars().take(2000).collect());
        }
    }
    snapshot
}

pub async fn run(app: &AppHandle, session: &SshSession, server: &str) {
    let slots = Semaphore::new(3);
    let presence = Arc::new(Mutex::new(String::new()));
    let presence_refresh = presence.clone();
    let refresh = async {
        loop {
            if let Ok(out) = execute(session, &slots, scripts::PRESENCE).await {
                *presence_refresh.lock().await = out.text;
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    };
    // Each loop has its own clock and error boundary. No infrastructure work is awaited by system polling.
    tokio::join!(
        refresh,
        worker(app, session, server, &slots, "mysql", 30, presence.clone()),
        worker(app, session, server, &slots, "postgres", 30, presence.clone()),
        worker(app, session, server, &slots, "mongo", 30, presence.clone()),
        worker(app, session, server, &slots, "rabbitmq", 30, presence.clone()),
        worker(app, session, server, &slots, "kafka", 60, presence.clone()),
        worker(app, session, server, &slots, "haproxy", 30, presence.clone()),
        worker(app, session, server, &slots, "nginx_upstreams", 60, presence.clone()),
        worker(app, session, server, &slots, "traefik", 30, presence.clone()),
        worker(app, session, server, &slots, "caddy", 30, presence.clone()),
        worker(app, session, server, &slots, "docker_infra", 30, presence.clone()),
        worker(app, session, server, &slots, "kubernetes", 60, presence.clone()),
        worker(app, session, server, &slots, "certificates", 3600, presence.clone()),
        worker(app, session, server, &slots, "cron", 300, presence.clone()),
        worker(app, session, server, &slots, "capacity", 300, presence.clone()),
        worker(app, session, server, &slots, "ssh_security", 60, presence.clone()),
        worker(app, session, server, &slots, "ports", 60, presence.clone()),
        worker(app, session, server, &slots, "updates", 1800, presence.clone()),
        worker(app, session, server, &slots, "fail2ban", 60, presence.clone()),
        worker(app, session, server, &slots, "last_logins", 300, presence.clone())
    );
}
async fn worker(
    app: &AppHandle,
    session: &SshSession,
    server: &str,
    slots: &Semaphore,
    name: &str,
    seconds: u64,
    presence: Arc<Mutex<String>>,
) {
    let mut previous: Option<Snapshot> = None;
    let mut generation = 0;
    loop {
        let next = poll(app, session, server, slots, name, previous.as_ref(), &presence).await;
        app.state::<InfraState>().put(server, next.clone());
        previous = Some(next);
        for _ in 0..seconds {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let current = app.state::<InfraState>().generation(server);
            if generation != current {
                generation = current;
                break;
            }
        }
    }
}
async fn poll(
    app: &AppHandle,
    session: &SshSession,
    server: &str,
    slots: &Semaphore,
    name: &str,
    previous: Option<&Snapshot>,
    presence: &Mutex<String>,
) -> Snapshot {
    let target = {
        let state = app.state::<AppState>();
        let db = state.metrics.lock();
        store::target(db.connection(), server, name)
    };
    let credentials = match if ["mysql", "postgres", "mongo", "rabbitmq"].contains(&name) {
        super::credentials(server, name)
    } else {
        Ok(Credentials::default())
    } {
        Ok(c) => c,
        Err(_) => {
            return Snapshot::problem(
                name,
                Phase::AuthNeeded,
                "Windows Credential Manager could not be read",
                "Check Credential Manager access and save the optional credentials again.",
            )
        }
    };
    if let Some(snapshot) = presence::gate(name, &presence.lock().await) {
        return snapshot;
    }
    let command = match name {
        "mysql" | "postgres" | "mongo" => scripts::database(name, &target, &credentials),
        "rabbitmq" => scripts::rabbitmq(&credentials, &target),
        "kafka" => scripts::KAFKA.into(),
        "haproxy" => scripts::HAPROXY.into(),
        "nginx_upstreams" => scripts::NGINX.into(),
        "traefik" => scripts::TRAEFIK.into(),
        "caddy" => scripts::CADDY.into(),
        "docker_infra" => scripts::DOCKER.into(),
        "kubernetes" => scripts::KUBERNETES.into(),
        "certificates" => scripts::CERTIFICATES.into(),
        "cron" => scripts::CRON.into(),
        "capacity" => scripts::CAPACITY.into(),
        "ssh_security" => scripts::SSH_SECURITY.into(),
        "ports" => scripts::PORTS.into(),
        "updates" => scripts::UPDATES.into(),
        "fail2ban" => scripts::FAIL2BAN.into(),
        "last_logins" => scripts::LAST_LOGINS.into(),
        _ => return failure(name, "not installed", 127),
    };
    // Discovery is independent of openssl/file access: PC TLS and URL suggestions
    // must remain usable when the remote certificate collector is unavailable.
    let certificate_config = if name == "certificates" {
        execute(session, slots, scripts::DOMAINS).await.ok()
    } else {
        None
    };
    if name == "certificates" {
        let mut domains = certificate_config
            .as_ref()
            .map(|c| parsers::domains(&c.text))
            .unwrap_or_default();
        domains.extend(
            app.state::<AppState>()
                .apps_for_server(server)
                .iter()
                .flat_map(|a| a.domains.iter())
                .filter_map(|d| parsers::valid_domain(d)),
        );
        domains.sort();
        domains.dedup();
        if !domains.is_empty() || certificate_config.is_some() {
            let state = app.state::<AppState>();
            let db = state.metrics.lock();
            let _ = store::save_domains(db.connection(), server, &domains);
        }
    }
    let mut output = match execute(session, slots, &command).await {
        Ok(o) => o,
        Err(e) => return failure(name, &e, 0),
    };
    if name == "nginx_upstreams" && presence::nginx_needs_root(&format!("{} {}", output.error, output.text)) {
        if let Ok(files) = execute(session, slots, scripts::NGINX_FILES).await {
            output.error = format!(
                "nginx -T needs root. Reading readable files in /etc/nginx/sites-enabled and conf.d. {}",
                files.error
            );
            output.text = files.text;
            if !output.text.trim().is_empty() {
                output.code = 0;
            }
        }
    }
    // MySQL 8.4 removed the legacy SLAVE spelling; MariaDB and older MySQL accept it.
    if name == "mysql" && output.code != 0 && output.text.contains("Threads_connected") {
        let modern = scripts::mysql_modern(&target, &credentials);
        if let Ok(retry) = execute(session, slots, &modern).await {
            if retry.code == 0 {
                output = retry;
            }
        }
    }
    // Errors are categorized, never echoed. Neither credentials nor full configurations enter diagnostics.
    if output.code != 0 && output.text.trim().is_empty() {
        return failure(name, &output.error, output.code);
    }
    let now = now_ms();
    let parsed = match name {
        "mysql" | "postgres" | "mongo" => parsers::database(name, &output.text),
        "rabbitmq" => parsers::rabbitmq(&output.text),
        "kafka" => Ok(parsers::kafka(&output.text)),
        "haproxy" => Ok(parsers::haproxy(&output.text)),
        "nginx_upstreams" => Ok(nginx::upstreams(&output.text)),
        "traefik" | "caddy" => parsers::proxy_api(name, &output.text),
        "docker_infra" => parsers::docker(&output.text),
        "kubernetes" => parsers::kubernetes(&output.text),
        "certificates" => Ok(parsers::certificates(&output.text, now)),
        "cron" => {
            let (cron_text, timers) = output
                .text
                .split_once("__TIMERS__")
                .unwrap_or((&output.text, ""));
            let mut rows = cron::parse(cron_text, now);
            rows.extend(cron::timers(timers));
            Ok(rows)
        }
        "capacity" => Ok(parsers::disks(&output.text)),
        "ssh_security" => Ok(parsers::ssh_failures(&output.text)),
        "ports" => Ok(parsers::ports(&output.text)),
        "updates" => Ok(parsers::updates(&output.text)),
        "fail2ban" => Ok(parsers::fail2ban(&output.text)),
        "last_logins" => Ok(parsers::last_logins(&output.text)),
        _ => Ok(vec![]),
    };
    let mut rows = match parsed {
        Ok(rows) => rows,
        Err(_) => {
            return failure(
                name,
                &format!("{} {}", output.error, output.text),
                output.code,
            )
        }
    };
    if name == "nginx_upstreams" && rows.is_empty() && !output.text.trim().is_empty() {
        let mut snap = Snapshot::problem(
            name,
            Phase::NotConfigured,
            "Nginx is installed. No upstream blocks were found in the readable config.",
            "Add an upstream block, run nginx -t, then reload nginx.",
        );
        if output.error.contains("needs root") {
            snap.message = "nginx -T needs root. Readable files in /etc/nginx/sites-enabled and conf.d have no upstream blocks.".into();
        }
        return snap;
    }
    if rows.is_empty() && (output.code != 0 || !output.error.trim().is_empty()) {
        return failure(
            name,
            &format!("{} {}", output.error, output.text),
            output.code,
        );
    }
    if ["mysql", "postgres", "mongo"].contains(&name) {
        for row in &mut rows {
            if let Some(old) = previous
                .filter(|p| p.phase == Phase::Ok || p.phase == Phase::Partial)
                .and_then(|p| p.rows.iter().find(|r| r.id == row.id))
            {
                let elapsed = (now - previous.unwrap().sampled_at_ms) as f64 / 1000.;
                let reset = row
                    .number("uptimeSeconds")
                    .zip(old.number("uptimeSeconds"))
                    .map(|(a, b)| a < b)
                    .unwrap_or(false)
                    || row.fields.get("counterEpoch") != old.fields.get("counterEpoch");
                if let (Some(new), Some(old)) =
                    (row.number("queryCounter"), old.number("queryCounter"))
                {
                    if new >= old && elapsed > 0. && !reset {
                        row.set("qps", (new - old) / elapsed);
                    }
                }
            }
        }
    }
    if name == "kafka" {
        for row in &mut rows {
            if let Some(consumer) = row.fields.get("consumer").and_then(|v| v.as_str()) {
                row.set("consumers", if consumer == "-" { 0 } else { 1 });
            }
            if let Some(old) = previous.and_then(|p| p.rows.iter().find(|r| r.id == row.id)) {
                let seconds = (now - previous.unwrap().sampled_at_ms) as f64 / 1000.;
                for (metric, counter) in [
                    ("consumePerSecond", "currentOffset"),
                    ("producePerSecond", "logEndOffset"),
                ] {
                    if let (Some(new), Some(before)) = (row.number(counter), old.number(counter)) {
                        if seconds > 0. && new >= before {
                            row.set(metric, (new - before) / seconds);
                        }
                    }
                }
            }
        }
    }
    if name == "capacity" {
        let state = app.state::<AppState>();
        let db = state.metrics.lock();
        if store::save_disk(db.connection(), server, &rows, now).is_err() {
            return failure(name, "Cannot persist capacity history", 0);
        }
        for row in &mut rows {
            if let Ok(samples) = store::disk_history(db.connection(), server, &row.name) {
                capacity::forecast(row, &samples);
                row.set("history",serde_json::json!(samples.iter().map(|(t,used,total)|serde_json::json!({"t":t,"v":if *total>0. {100.*used/total}else{0.}})).collect::<Vec<_>>()));
            }
        }
    }
    if name == "nginx_upstreams" {
        let paths = nginx::log_paths(&output.text);
        let command = paths
            .iter()
            .map(|path| format!("tail -n 500 -- {} 2>/dev/null", scripts::quote(path)))
            .collect::<Vec<_>>()
            .join(";");
        if !command.is_empty() {
            if let Ok(logs) = execute(session, slots, &command).await {
                nginx::add_log_health(&mut rows, &logs.text);
            }
        }
        probe_backends(session, slots, &mut rows).await;
    }
    if name == "caddy" {
        probe_backends(session, slots, &mut rows).await;
    }
    if name == "certificates" {
        if let Some(config) = certificate_config {
            // Certificate paths configured outside Let's Encrypt are read too.
            let re = regex::Regex::new(
                r#"(?im)^\s*(?:ssl_certificate|SSLCertificateFile)\s+["']?(/[^;\s"']+)"#,
            )
            .unwrap();
            let mut paths: Vec<String> = re
                .captures_iter(&config.text)
                .map(|c| c[1].into())
                .collect();
            paths.sort();
            paths.dedup();
            paths.truncate(128);
            for group in paths.chunks(16) {
                let command=group.iter().filter(|p|!rows.iter().any(|r|r.name==**p)).map(|p|format!("printf '__CERT__\\t%s\\n' {}; openssl x509 -in {} -noout -enddate -subject",scripts::quote(p),scripts::quote(p))).collect::<Vec<_>>().join(";");
                if !command.is_empty() {
                    if let Ok(cert) = execute(session, slots, &command).await {
                        rows.extend(parsers::certificates(&cert.text, now));
                    }
                }
            }
        }
    }
    let mut snapshot = Snapshot::ok(name, rows);
    if output.code != 0 || !output.error.trim().is_empty() || output.truncated {
        snapshot.phase = Phase::Partial;
        snapshot.message = if output.truncated {
            "Output reached the 512 KiB limit. This is a partial sample.".into()
        } else {
            "Some read operations failed. The available metrics are shown.".into()
        };
        snapshot.fix_hint = failure(name, &output.error, output.code).fix_hint;
    }
    if name == "cron"
        && (output.text.contains("Operation not permitted")
            || output.text.contains("Permission denied")
            || output.text.contains("must be privileged"))
    {
        snapshot.phase = Phase::Partial;
        snapshot.message =
            "Only readable crontabs and timer results are shown. Other users denied access.".into();
        snapshot.fix_hint = failure(name, "permission denied", 1).fix_hint;
    }
    if name == "updates" {
        snapshot.message = format!(
            "{} pending updates · {} security · existing package cache",
            snapshot.rows.len(),
            snapshot
                .rows
                .iter()
                .filter(|r| r.state == "security")
                .count()
        );
    }
    if name == "rabbitmq" {
        snapshot.message =
            "Queue sample up to 500 queues. CLI covers up to 32 vhosts. Rates require the management API."
                .into();
    }
    if name == "ssh_security" {
        snapshot.message="Failed authentication log events, bounded to 10,000 lines. One attempt may produce multiple events. Journal covers the last 24 hours. File fallback uses recent lines.".into();
    }
    snapshot
}

async fn probe_backends(session: &SshSession, slots: &Semaphore, rows: &mut [Resource]) {
    // One bounded remote batch, up to 32 probes; remaining rows explicitly retain unknown reachability.
    let mut command = String::new();
    for (index, row) in rows.iter().enumerate().take(32) {
        if row.fields.get("configuredDown").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }
        let raw = row
            .fields
            .get("address")
            .and_then(|v| v.as_str())
            .unwrap_or(&row.name);
        let raw = raw
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_end_matches('/');
        if let Some((host, port)) = raw.rsplit_once(':') {
            let host = host.trim_start_matches('[').trim_end_matches(']');
            if host
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".-:".contains(&c))
                && port.parse::<u16>().is_ok()
            {
                command.push_str(&format!("(if command -v nc >/dev/null 2>&1; then nc -z -w 2 {} {}; else timeout 2 bash -c {}; fi) >/dev/null 2>&1; printf '{index}:%s\\n' \"$?\"; ",scripts::quote(host),scripts::quote(port),scripts::quote(&format!("exec 3<>/dev/tcp/{host}/{port}"))));
            }
        }
    }
    if command.is_empty() {
        return;
    }
    if let Ok(output) = execute(session, slots, &command).await {
        for line in output.text.lines() {
            if let Some((i, code)) = line.split_once(':') {
                if let (Ok(index), Ok(code)) = (i.parse::<usize>(), code.parse::<u32>()) {
                    if let Some(row) = rows.get_mut(index) {
                        row.set(
                            "tcp",
                            if code == 0 {
                                "up"
                            } else if code == 127 {
                                "unavailable"
                            } else {
                                "down"
                            },
                        );
                        if code != 127 {
                            row.state = if code == 0 { "up" } else { "down" }.into();
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_error_states_and_no_secret_echo() {
        assert_eq!(
            failure("mysql", "Access denied password secret", 1).phase,
            Phase::AuthNeeded
        );
        assert!(!failure("mysql", "secret", 1).message.contains("secret"));
        assert_eq!(
            failure("docker_infra", "permission denied", 1).phase,
            Phase::PermissionDenied
        );
        assert_eq!(failure("mongo", "missing", 127).phase, Phase::NotInstalled);
        assert_eq!(failure("mongo", "", 124).phase, Phase::TimedOut);
        assert!(!failure("haproxy", "boom", 1).message.contains("unsupported format"));
        let err = failure("haproxy", "parser blew up", 1);
        assert_eq!(err.phase, Phase::Error);
        assert_eq!(err.detail.as_deref(), Some("parser blew up"));
    }
}

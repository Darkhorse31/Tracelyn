//! DNS, HTTP and TLS execute on the Windows PC. No HTTP check is sent through SSH.
use super::{model::*, parsers, store, InfraState};
use crate::{metrics_ingest::now_ms, state::AppState};
use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddr, TcpStream},
    process::Command,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};
use url::Url;

pub fn validate_url(raw: &str) -> Result<Url, String> {
    if raw.len() > 2048 {
        return Err("URL must be at most 2048 characters".into());
    }
    let url = Url::parse(raw).map_err(|_| "Enter an absolute http:// or https:// URL")?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("Use an HTTP(S) URL without embedded credentials or a fragment".into());
    }
    Ok(url)
}
fn hidden_command(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}
async fn resolve(host: &str, port: u16) -> Result<(Vec<SocketAddr>, f64), String> {
    let start = Instant::now();
    let addresses = tokio::time::timeout(
        Duration::from_secs(4),
        tokio::net::lookup_host((host, port)),
    )
    .await
    .map_err(|_| "DNS resolution timed out after 4 seconds")?
    .map_err(|_| "DNS resolution failed")?;
    let mut addresses: Vec<_> = addresses.take(16).collect();
    addresses.sort();
    addresses.dedup();
    if addresses.is_empty() {
        return Err("DNS returned no addresses".into());
    }
    Ok((addresses, start.elapsed().as_secs_f64() * 1000.))
}
pub async fn check(raw: &str) -> CheckResult {
    let start = Instant::now();
    let mut result = CheckResult {
        ts_ms: now_ms(),
        ok: false,
        status_code: None,
        addresses: vec![],
        dns_ms: None,
        connect_ms: None,
        tls_ms: None,
        ttfb_ms: None,
        total_ms: 0.,
        error: None,
    };
    let url = match validate_url(raw) {
        Ok(url) => url,
        Err(err) => {
            result.error = Some(err);
            return result;
        }
    };
    match resolve(
        url.host_str().unwrap_or(""),
        url.port_or_known_default().unwrap_or(443),
    )
    .await
    {
        Ok((addresses, dns)) => {
            result.addresses = addresses.iter().map(ToString::to_string).collect();
            result.dns_ms = Some(dns);
        }
        Err(err) => {
            result.total_ms = start.elapsed().as_secs_f64() * 1000.;
            result.error = Some(err);
            return result;
        }
    }
    let raw = url.to_string();
    let response = tauri::async_runtime::spawn_blocking(move || {
        hidden_command("curl.exe")
            .args([
                "--disable",
                "--silent",
                "--show-error",
                "--noproxy",
                "*",
                "--connect-timeout",
                "4",
                "--max-time",
                "10",
                "--max-filesize",
                "1048576",
                "--output",
                "NUL",
                "--write-out",
                "%{json}",
                "--proto",
                "=http,https",
                "--url",
                &raw,
            ])
            .output()
    })
    .await;
    match response {
        Ok(Ok(output)) => {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                apply_timings(&mut result, &v);
            }
            result.ok = output.status.success()
                && result
                    .status_code
                    .map(|s| (200..400).contains(&s))
                    .unwrap_or(false);
            if !result.ok {
                result.error = Some(if output.status.success() {
                    format!("HTTP {} response", result.status_code.unwrap_or(0))
                } else {
                    match output.status.code().unwrap_or(-1) {
                        6 => "HTTP DNS lookup failed",
                        7 => "TCP connection failed",
                        28 => "HTTP check timed out after 10 seconds",
                        60 => "TLS certificate validation failed",
                        63 => "Response exceeds the 1 MiB check limit",
                        _ => "HTTP GET failed. Check connectivity and TLS",
                    }
                    .into()
                });
            }
        }
        _ => {
            result.error = Some(
                "Windows curl.exe could not run. Ensure the built-in curl is available on PATH."
                    .into(),
            )
        }
    }
    result.total_ms = start.elapsed().as_secs_f64() * 1000.;
    result
}
pub fn apply_timings(result: &mut CheckResult, v: &serde_json::Value) {
    let seconds = |key: &str| v[key].as_f64().unwrap_or(0.);
    result.status_code = v["http_code"].as_u64().filter(|n| *n > 0).map(|n| n as u16);
    let connect = seconds("time_connect");
    let tls = seconds("time_appconnect");
    let first = seconds("time_starttransfer");
    if connect > 0. {
        result.connect_ms = Some(((connect - seconds("time_namelookup")) * 1000.).max(0.));
    }
    if tls > 0. {
        result.tls_ms = Some(((tls - connect) * 1000.).max(0.));
    }
    if first > 0. {
        result.ttfb_ms = Some(((first - seconds("time_pretransfer")) * 1000.).max(0.));
    }
}

async fn certificate(domain: &str) -> Resource {
    let mut row = Resource::new(domain, "unknown");
    row.set("source", "PC TLS :443");
    let addresses = match resolve(domain, 443).await {
        Ok((addresses, _)) => addresses,
        Err(err) => {
            row.set("note", err);
            return row;
        }
    };
    let name = domain.to_string();
    let outcome =
        tauri::async_runtime::spawn_blocking(move || inspect_certificate(&name, &addresses)).await;
    match outcome {
        Ok(Ok((expiry, subject, verified))) => {
            parsers::certificate_state(&mut row, expiry, now_ms());
            row.set("subject", subject);
            row.set("verified", verified);
            if !verified {
                row.state = "critical".into();
                row.set("note","TLS trust or hostname validation failed. Expiry was inspected with a separate unverified handshake.");
            }
        }
        _ => {
            row.state = "error".into();
            row.set("note","Could not inspect the public TLS certificate. Check DNS, TCP 443 and TLS/SNI. Wildcard names need a concrete hostname.");
        }
    }
    row
}
struct DeadlineStream {
    stream: TcpStream,
    deadline: Instant,
}
impl DeadlineStream {
    fn remaining(&self) -> std::io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .map(|d| d.min(Duration::from_secs(3)))
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "TLS inspection exceeded its total deadline",
                )
            })
    }
}
impl std::io::Read for DeadlineStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.stream.set_read_timeout(Some(self.remaining()?))?;
        std::io::Read::read(&mut self.stream, buffer)
    }
}
impl std::io::Write for DeadlineStream {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        std::io::Write::write(&mut self.stream, buffer)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.remaining()?;
        std::io::Write::flush(&mut self.stream)
    }
}
fn inspect_certificate(
    host: &str,
    addresses: &[SocketAddr],
) -> Result<(i64, String, bool), String> {
    let deadline = Instant::now() + Duration::from_secs(12);
    let open = || -> Result<DeadlineStream, String> {
        for address in addresses.iter().take(2) {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or("TLS inspection timed out")?;
            if let Ok(stream) =
                TcpStream::connect_timeout(address, remaining.min(Duration::from_secs(3)))
            {
                return Ok(DeadlineStream { stream, deadline });
            }
        }
        Err("TCP connection failed".into())
    };
    let connector = native_tls::TlsConnector::new().map_err(|_| "TLS initialization failed")?;
    let (stream, verified) = match connector.connect(host, open()?) {
        Ok(stream) => (stream, true),
        Err(_) => {
            let connector = native_tls::TlsConnector::builder()
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hostnames(true)
                .build()
                .map_err(|_| "TLS initialization failed")?;
            (
                connector
                    .connect(host, open()?)
                    .map_err(|_| "TLS handshake failed")?,
                false,
            )
        }
    };
    let cert = stream
        .peer_certificate()
        .map_err(|_| "Peer certificate unavailable")?
        .ok_or("Peer certificate missing")?;
    let der = cert.to_der().map_err(|_| "Invalid certificate encoding")?;
    let (_, cert) =
        x509_parser::parse_x509_certificate(&der).map_err(|_| "Invalid X.509 certificate")?;
    Ok((
        cert.validity().not_after.timestamp() * 1000,
        cert.subject().to_string(),
        verified,
    ))
}

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut due: HashMap<String, i64> = HashMap::new();
        let mut inflight = HashSet::new();
        let mut jobs = tokio::task::JoinSet::new();
        loop {
            while let Some(done) = jobs.try_join_next() {
                if let Ok(key) = done {
                    inflight.remove(&key);
                }
            }
            let (checks, servers) = {
                let state = app.state::<AppState>();
                let checks =
                    store::checks(state.metrics.lock().connection(), None).unwrap_or_default();
                let servers = state.config.lock().list_servers().to_vec();
                (checks, servers)
            };
            let valid_servers: HashSet<_> = servers.iter().map(|s| s.id.clone()).collect();
            app.state::<InfraState>().prune(&valid_servers);
            let active_keys: HashSet<_> = checks
                .iter()
                .map(|c| format!("check:{}", c.id))
                .chain(servers.iter().flat_map(|s| {
                    let state = app.state::<AppState>();
                    let domains = store::domains(state.metrics.lock().connection(), &s.id)
                        .unwrap_or_default();
                    domains
                        .into_iter()
                        .map(move |d| format!("cert:{}:{d}", s.id))
                }))
                .collect();
            due.retain(|key, _| active_keys.contains(key));
            let mut ordered_checks: Vec<_> = checks
                .iter()
                .filter(|c| c.enabled && valid_servers.contains(&c.server_id))
                .collect();
            ordered_checks
                .sort_by_key(|c| due.get(&format!("check:{}", c.id)).copied().unwrap_or(0));
            for check in ordered_checks {
                let key = format!("check:{}", check.id);
                if jobs.len() >= 3 {
                    break;
                }
                if inflight.contains(&key) || due.get(&key).copied().unwrap_or(0) > now_ms() {
                    continue;
                }
                due.insert(key.clone(), now_ms() + check.interval_seconds as i64 * 1000);
                inflight.insert(key.clone());
                let check = check.clone();
                let handle = app.clone();
                jobs.spawn(async move {
                    let slots = handle.state::<InfraState>().local_slots.clone();
                    let Ok(_permit) = slots.acquire_owned().await else {
                        return key;
                    };
                    let result = check_url(&check.url).await;
                    let saved = {
                        let state = handle.state::<AppState>();
                        let db = state.metrics.lock();
                        let exists = db
                            .connection()
                            .query_row(
                                "SELECT count(*) FROM infra_checks WHERE id=?1",
                                [&check.id],
                                |r| r.get::<_, i64>(0),
                            )
                            .unwrap_or(0)
                            > 0;
                        exists && store::save_result(db.connection(), &check.id, &result).is_ok()
                    };
                    let mut row = Resource::new(&check.url, if result.ok { "up" } else { "down" });
                    row.id = check.id.clone();
                    row.set("totalMs", result.total_ms);
                    row.set(
                        "statusCode",
                        result
                            .status_code
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                    );
                    handle
                        .state::<InfraState>()
                        .put_uptime(&check.server_id, row, saved);
                    key
                });
            }
            for server in &servers {
                let domains = {
                    let state = app.state::<AppState>();
                    let db = state.metrics.lock();
                    store::domains(db.connection(), &server.id).unwrap_or_default()
                };
                app.state::<InfraState>().sync_local(
                    &server.id,
                    &checks
                        .iter()
                        .filter(|c| c.server_id == server.id)
                        .cloned()
                        .collect::<Vec<_>>(),
                    &domains,
                );
                for domain in domains {
                    if jobs.len() >= 4 {
                        break;
                    }
                    let key = format!("cert:{}:{domain}", server.id);
                    if inflight.contains(&key) || due.get(&key).copied().unwrap_or(0) > now_ms() {
                        continue;
                    }
                    due.insert(key.clone(), now_ms() + 6 * 3_600_000);
                    inflight.insert(key.clone());
                    let handle = app.clone();
                    let server = server.id.clone();
                    jobs.spawn(async move {
                        let slots = handle.state::<InfraState>().local_slots.clone();
                        let Ok(_permit) = slots.acquire_owned().await else {
                            return key;
                        };
                        let row = certificate(&domain).await;
                        handle.state::<InfraState>().put_certificate(&server, row);
                        key
                    });
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}
async fn check_url(url: &str) -> CheckResult {
    check(url).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls_are_http_only_and_secrets_do_not_enter_history() {
        assert!(validate_url("https://example.com/health").is_ok());
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("https://user:secret@example.com").is_err());
        assert!(validate_url("https://example.com/#secret").is_err());
    }
    #[test]
    fn tls_reads_obey_the_total_deadline() {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (_peer, _) = listener.accept().unwrap();
        let mut stream = DeadlineStream {
            stream,
            deadline: Instant::now() - Duration::from_millis(1),
        };
        assert_eq!(
            stream.read(&mut [0u8; 1]).unwrap_err().kind(),
            std::io::ErrorKind::TimedOut
        );
    }
    #[test]
    fn curl_timings_are_disjoint_phases() {
        let mut r = CheckResult {
            ts_ms: 0,
            ok: false,
            status_code: None,
            addresses: vec![],
            dns_ms: None,
            connect_ms: None,
            tls_ms: None,
            ttfb_ms: None,
            total_ms: 0.,
            error: None,
        };
        apply_timings(
            &mut r,
            &serde_json::json!({"http_code":200,"time_namelookup":0.01,"time_connect":0.04,"time_appconnect":0.09,"time_pretransfer":0.10,"time_starttransfer":0.15}),
        );
        assert_eq!(r.status_code, Some(200));
        assert!((r.connect_ms.unwrap() - 30.).abs() < 0.001);
        assert!((r.tls_ms.unwrap() - 50.).abs() < 0.001);
        assert!((r.ttfb_ms.unwrap() - 50.).abs() < 0.001);
    }
    #[tokio::test]
    async fn pc_get_resolves_dns_and_records_http_success_and_failure() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            for status in ["200 OK", "503 Service Unavailable"] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buffer = [0u8; 2048];
                let count = socket.read(&mut buffer).await.unwrap();
                assert!(std::str::from_utf8(&buffer[..count])
                    .unwrap()
                    .starts_with("GET /health HTTP/1.1"));
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 {status}\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK"
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let success = check(&format!("http://localhost:{port}/health")).await;
        assert!(success.ok, "{:?}", success.error);
        assert_eq!(success.status_code, Some(200));
        assert!(success.dns_ms.is_some());
        assert!(success.connect_ms.is_some());
        assert!(success.tls_ms.is_none());
        let failure = check(&format!("http://localhost:{port}/health")).await;
        assert!(!failure.ok);
        assert_eq!(failure.status_code, Some(503));
        assert_eq!(failure.error.as_deref(), Some("HTTP 503 response"));
        server.await.unwrap();
    }
}

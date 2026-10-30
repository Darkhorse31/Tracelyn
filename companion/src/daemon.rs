use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tracelyn_companion::{
    notify::{self, ServerChannel},
    storage, SOCKET, VERSION,
};
use tracelyn_core::{logic::Gate, types::IntelRule};
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
}
fn configuration_hash() -> String {
    let mut hash = Sha256::new();
    for name in ["rules.json", "channels.json", "watch.json"] {
        hash.update(read(&format!("/etc/tracelyn/{name}")).as_bytes());
    }
    hex::encode(hash.finalize())
}
#[derive(Default, Serialize, Deserialize)]
struct Runtime {
    firing: bool,
    condition: Option<i64>,
    clear: Option<i64>,
    last_sent: Option<i64>,
    #[serde(default)]
    last_attempt: Option<i64>,
}
fn args_request(args: &[String]) -> Result<Value, String> {
    let mut request = json!({"command":args.first().map(String::as_str).unwrap_or("status"),"since":now()-86_400_000,"until":now(),"metric":""});
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {}
            "--since" | "--until" => {
                let k = args[i].trim_start_matches("--");
                i += 1;
                request[k] = json!(args
                    .get(i)
                    .ok_or("Missing timestamp")?
                    .parse::<i64>()
                    .map_err(|_| "Use Unix milliseconds")?);
            }
            "--metric" => {
                i += 1;
                request["metric"] = json!(args.get(i).ok_or("Missing metric")?);
            }
            _ => return Err("Unknown argument".into()),
        }
        i += 1;
    }
    Ok(request)
}
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("version") {
        println!("{VERSION}");
        return Ok(());
    }
    if args.first().map(String::as_str) != Some("serve") {
        let request = args_request(&args)?;
        let mut stream = UnixStream::connect(SOCKET)?;
        stream.set_read_timeout(Some(Duration::from_secs(15)))?;
        writeln!(stream, "{request}")?;
        let mut out = String::new();
        stream.take(16 * 1024 * 1024).read_to_string(&mut out)?;
        println!("{out}");
        return Ok(());
    }
    fs::create_dir_all("/var/lib/tracelyn")?;
    let db = Arc::new(Mutex::new(storage::open(
        "/var/lib/tracelyn/history.sqlite",
    )?));
    // Remove only a stale socket, never arbitrary files at the socket path.
    if fs::symlink_metadata(SOCKET).is_ok() {
        use std::os::unix::fs::FileTypeExt;
        if !fs::symlink_metadata(SOCKET)?.file_type().is_socket() {
            return Err("Socket path is not a socket".into());
        }
        if UnixStream::connect(SOCKET).is_ok() {
            return Err("Companion already running".into());
        }
        fs::remove_file(SOCKET)?;
    }
    let listener = UnixListener::bind(SOCKET)?;
    fs::set_permissions(SOCKET, fs::Permissions::from_mode(0o660))?;
    let start = Instant::now();
    let socket_db = db.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
            let mut line = String::new();
            if BufReader::new((&stream).take(8192))
                .read_line(&mut line)
                .is_err()
            {
                continue;
            }
            let response=(||->Result<Value,Box<dyn std::error::Error>> {
                let r:Value=serde_json::from_str(&line)?;
                let db=socket_db.lock().map_err(|_|"Database unavailable")?;
                let since=r["since"].as_i64().unwrap_or(now()-86_400_000);
                let until=r["until"].as_i64().unwrap_or(now());
                Ok(match r["command"].as_str().unwrap_or("status") {
                    "status"=>json!({"state":"running","version":VERSION,"rulesHash":configuration_hash(),"uptimeSecs":start.elapsed().as_secs(),"heartbeatAgeMs":now().saturating_sub(read("/var/lib/tracelyn/heartbeat").trim().parse::<i64>().unwrap_or(0)),"lastHeartbeat":read("/var/lib/tracelyn/heartbeat").trim().parse::<i64>().ok(),"diskUsed":fs::read_dir("/var/lib/tracelyn")?.filter_map(Result::ok).filter_map(|e|e.metadata().ok()).map(|m|m.len()).sum::<u64>()}),
                    "query"=>json!(storage::query(&db,since,until,r["metric"].as_str().unwrap_or(""),now())?),
                    "events"=>{let mut q=db.prepare("SELECT body FROM events WHERE ts>=?1 AND ts<=?2 ORDER BY ts LIMIT 5000")?;let rows=q.query_map([since,until],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;json!(rows.iter().filter_map(|s|serde_json::from_str::<Value>(s).ok()).collect::<Vec<_>>())},
                    "processes"=>{let mut q=db.prepare("SELECT ts,body FROM processes WHERE ts>=?1 AND ts<=?2 ORDER BY ts DESC LIMIT 200")?;let rows=q.query_map([since,until],|r|Ok(json!({"tsMs":r.get::<_,i64>(0)?,"processes":serde_json::from_str::<Value>(&r.get::<_,String>(1)?).unwrap_or(Value::Null)})))?.collect::<Result<Vec<_>,_>>()?;json!(rows)},
                    _=>json!({"error":"Unknown command"})
                })
            })().unwrap_or_else(|_|json!({"error":"Request failed"}));
            let _ = writeln!(stream, "{response}");
        }
    });
    // Checks and outbound delivery have their own bounded cadence so slow endpoints
    // cannot stop the five second metric collector or socket heartbeat.
    let alert_db = db.clone();
    std::thread::spawn(move || alert_loop(alert_db));
    let mut prev_cpu = None;
    let mut prev_net = None;
    let mut prev_disk = None;
    let mut previous = Instant::now();
    let mut tick = 0u64;
    loop {
        let began = Instant::now();
        let ts = now();
        let elapsed = previous.elapsed().as_secs_f64();
        previous = Instant::now();
        let mut values = HashMap::new();
        if let Some(cpu) = tracelyn_core::proc_stat::parse_cpu_jiffies(&read("/proc/stat")) {
            if let Some(prev) = prev_cpu {
                values.insert(
                    "cpu.percent".into(),
                    tracelyn_core::proc_stat::cpu_usage_percent(prev, cpu),
                );
            }
            prev_cpu = Some(cpu);
        }
        if let Some(v) = tracelyn_core::proc_meminfo::memory_used_percent(&read("/proc/meminfo")) {
            values.insert("memory.percent".into(), v);
        }
        if let Some((a, b, c)) = tracelyn_core::proc_loadavg::parse_loadavg(&read("/proc/loadavg"))
        {
            for (k, v) in [("load.1", a), ("load.5", b), ("load.15", c)] {
                values.insert(k.into(), v);
            }
        }
        if let Some(net) = tracelyn_core::proc_net_dev::parse_net_bytes(&read("/proc/net/dev")) {
            if let Some(prev) = prev_net {
                let (a, b) = tracelyn_core::proc_net_dev::net_rates_bps(prev, net, elapsed);
                values.insert("net.rx_bps".into(), a);
                values.insert("net.tx_bps".into(), b);
            }
            prev_net = Some(net);
        }
        if let Some(disk) =
            tracelyn_core::proc_diskstats::parse_disk_sectors(&read("/proc/diskstats"))
        {
            if let Some(prev) = prev_disk {
                let (a, b) = tracelyn_core::proc_diskstats::disk_rates_bps(prev, disk, elapsed);
                values.insert("disk.read_bps".into(), a);
                values.insert("disk.write_bps".into(), b);
            }
            prev_disk = Some(disk);
        }
        // Capacity is separate from disk IO counters.
        unsafe {
            let mut stat: libc::statvfs = std::mem::zeroed();
            if libc::statvfs(c"/".as_ptr(), &mut stat) == 0 && stat.f_blocks > 0 {
                values.insert(
                    "disk.percent".into(),
                    100. * (stat.f_blocks - stat.f_bavail) as f64 / stat.f_blocks as f64,
                );
            }
        }
        {
            let db = db.lock().unwrap();
            if let Err(e) = storage::insert(&db, ts, &values) {
                eprintln!("Metric storage failed: {e}");
            }
            if tick % 12 == 0 {
                let _ = storage::prune(&db, ts);
            }
        }
        if tick % 6 == 0 {
            let processes = top_processes();
            let _ = db.lock().unwrap().execute(
                "INSERT OR REPLACE INTO processes VALUES (?1,?2)",
                rusqlite::params![ts, processes.to_string()],
            );
        }
        fs::write("/var/lib/tracelyn/heartbeat.next", ts.to_string())?;
        fs::rename(
            "/var/lib/tracelyn/heartbeat.next",
            "/var/lib/tracelyn/heartbeat",
        )?;
        tick += 1;
        std::thread::sleep(Duration::from_secs(5).saturating_sub(began.elapsed()));
    }
}
fn top_processes() -> Value {
    let mut rows = Vec::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for e in entries.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            let s = read(&format!("/proc/{pid}/stat"));
            let Some((name, rest)) = s.rsplit_once(')') else {
                continue;
            };
            let cols: Vec<_> = rest.split_whitespace().collect();
            let rss = cols
                .get(21)
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let cpu = cols
                .get(11)
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0)
                + cols
                    .get(12)
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
            rows.push((rss,json!({"pid":pid,"name":name.split_once('(').map(|(_,n)|n).unwrap_or(""),"rssPages":rss,"cpuTicks":cpu})));
        }
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.0));
    json!(rows.into_iter().take(20).map(|r| r.1).collect::<Vec<_>>())
}
fn alert_loop(db: Arc<Mutex<rusqlite::Connection>>) {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let mut last_log = now();
    loop {
        let ts = now();
        let rules: Vec<IntelRule> =
            serde_json::from_str(&read("/etc/tracelyn/rules.json")).unwrap_or_default();
        let channels: Vec<ServerChannel> = secure_channels().unwrap_or_default();
        for rule in rules.iter().filter(|r| r.enabled) {
            let result = check_rule(rule, &client, &db, last_log, ts);
            let Ok(value) = result else { continue };
            let previous: String = db
                .lock()
                .unwrap()
                .query_row("SELECT body FROM runtime WHERE rule=?1", [&rule.id], |r| {
                    r.get(0)
                })
                .unwrap_or_default();
            let mut state: Runtime = serde_json::from_str(&previous).unwrap_or_default();
            let old = state.firing;
            let gate = tracelyn_companion::evaluate(
                rule,
                Gate {
                    firing: state.firing,
                    condition_since_ms: state.condition,
                    clear_since_ms: state.clear,
                },
                value,
                ts,
            );
            state.firing = gate.firing;
            state.condition = gate.condition_since_ms;
            state.clear = gate.clear_since_ms;
            let config: Value =
                serde_json::from_str(&read("/etc/tracelyn/watch.json")).unwrap_or(json!({}));
            let offset = config["utcOffsetSecs"].as_i64().unwrap_or(0) as i32;
            let minute = tracelyn_core::policy::minute_of_day(ts, offset);
            let quiet = match (rule.params.quiet_start_min, rule.params.quiet_end_min) {
                (Some(a), Some(b)) => tracelyn_core::policy::in_quiet_hours(minute, a, b),
                _ => false,
            };
            let due = state
                .last_sent
                .map(|t| {
                    ts.saturating_sub(t)
                        >= (rule.params.cooldown_secs.min(i64::MAX as u64 / 1000) * 1000) as i64
                })
                .unwrap_or(true);
            let retry = state.last_sent.is_none()
                && state
                    .last_attempt
                    .map(|t| ts.saturating_sub(t) >= 30_000)
                    .unwrap_or(true);
            if old != state.firing || (!quiet && state.firing && retry) {
                let title = format!(
                    "{}: {}",
                    if state.firing { "Alert" } else { "Resolved" },
                    rule.name
                );
                let mut sent = Vec::new();
                let mut failed = Vec::new();
                for c in channels.iter().filter(|c| {
                    c.channel.enabled
                        && c.channel.kind != "windows"
                        && state.firing
                        && !quiet
                        && due
                }) {
                    if notify::send(&client, c, &title, &format!("Value: {value}")).is_ok() {
                        sent.push(c.channel.id.clone())
                    } else {
                        failed.push(c.channel.id.clone())
                    }
                }
                if !sent.is_empty() {
                    state.last_sent = Some(ts);
                }
                state.last_attempt = Some(ts);
                let event = json!({"id":format!("{}:{ts}",rule.id),"ruleId":rule.id,"title":title,"severity":rule.severity,"tsMs":ts,"value":value,"threshold":rule.params.threshold,"firing":state.firing,"sentBy":"server","sentChannels":sent,"failedChannels":failed});
                if old != state.firing || !sent.is_empty() || !failed.is_empty() {
                    let _ = db.lock().unwrap().execute(
                        "INSERT INTO events VALUES (?1,?2,?3)",
                        rusqlite::params![ts, rule.id, event.to_string()],
                    );
                }
            }
            let _ = db.lock().unwrap().execute(
                "INSERT OR REPLACE INTO runtime VALUES (?1,?2)",
                rusqlite::params![rule.id, serde_json::to_string(&state).unwrap()],
            );
        }
        last_log = ts;
        std::thread::sleep(Duration::from_secs(5));
    }
}
fn secure_channels() -> Result<Vec<ServerChannel>, Box<dyn std::error::Error>> {
    let file = fs::File::open("/etc/tracelyn/channels.json")?;
    if file.metadata()?.permissions().mode() & 0o777 != 0o600 {
        return Err("Channel file must have mode 600".into());
    }
    Ok(serde_json::from_reader(file.take(1024 * 1024))?)
}
fn check_rule(
    rule: &IntelRule,
    client: &reqwest::blocking::Client,
    db: &Arc<Mutex<rusqlite::Connection>>,
    since: i64,
    ts: i64,
) -> Result<f64, String> {
    let p = &rule.params;
    match rule.kind.as_str() {
        "uptime" if p.host.starts_with("https://") || p.host.starts_with("http://") => Ok(
            if client
                .get(&p.host)
                .send()
                .map(|r| r.status().is_success())
                .unwrap_or(false)
            {
                1.
            } else {
                0.
            },
        ),
        "uptime" => {
            use std::net::ToSocketAddrs;
            let addr = (p.host.as_str(), p.port)
                .to_socket_addrs()
                .map_err(|_| "DNS failed")?
                .next()
                .ok_or("No address")?;
            Ok(
                if std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(5)).is_ok() {
                    1.
                } else {
                    0.
                },
            )
        }
        "ssl_expiry" => ssl_days(&p.host, p.port, ts),
        "log_pattern" => {
            let regex = regex::RegexBuilder::new(&p.pattern)
                .size_limit(1024 * 1024)
                .build()
                .map_err(|_| "Invalid log pattern")?;
            let config: Value =
                serde_json::from_str(&read("/etc/tracelyn/watch.json")).unwrap_or(json!({}));
            let mut count = 0;
            // Journal query is time bounded and capped, with no follow subprocess.
            let output = std::process::Command::new("journalctl")
                .args([
                    "--no-pager",
                    "-q",
                    "-n",
                    "1000",
                    "--since",
                    &format!("@{}.{:03}", since.div_euclid(1000), since.rem_euclid(1000)),
                    "--until",
                    &format!("@{}.{:03}", ts.div_euclid(1000), ts.rem_euclid(1000)),
                    "-o",
                    "cat",
                ])
                .output()
                .map_err(|_| "Journal unavailable")?;
            count += String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|s| regex.is_match(s))
                .count();
            if let Some(paths) = config["files"].as_array() {
                for path in paths.iter().filter_map(Value::as_str) {
                    // Persistent offsets prevent repeated alerts for old lines. Rotation resets the offset.
                    use std::io::{Seek, SeekFrom};
                    use std::os::unix::fs::MetadataExt;
                    use std::os::unix::fs::OpenOptionsExt;
                    if let Ok(mut file) = fs::OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_NONBLOCK)
                        .open(path)
                    {
                        let meta = file.metadata().map_err(|_| "File metadata")?;
                        if !meta.is_file() {
                            continue;
                        }
                        let key = format!("file:{}:{path}", rule.id);
                        let old: String = db
                            .lock()
                            .unwrap()
                            .query_row("SELECT body FROM runtime WHERE rule=?1", [&key], |r| {
                                r.get(0)
                            })
                            .unwrap_or_default();
                        let state: Value = serde_json::from_str(&old).unwrap_or(Value::Null);
                        let offset = if state["inode"].as_u64() == Some(meta.ino()) {
                            let offset = state["offset"].as_u64().unwrap_or(0);
                            if offset > meta.len() {
                                0
                            } else {
                                offset
                            }
                        } else if state["inode"].is_u64() {
                            0
                        } else {
                            meta.len()
                        };
                        file.seek(SeekFrom::Start(offset)).map_err(|_| "Log seek")?;
                        let mut bytes = Vec::new();
                        let size = (&mut file)
                            .take(256 * 1024)
                            .read_to_end(&mut bytes)
                            .unwrap_or(0);
                        let text = String::from_utf8_lossy(&bytes);
                        count += text.lines().filter(|s| regex.is_match(s)).count();
                        let _ = db.lock().unwrap().execute(
                            "INSERT OR REPLACE INTO runtime VALUES (?1,?2)",
                            rusqlite::params![
                                key,
                                json!({"inode":meta.ino(),"offset":offset+size as u64}).to_string()
                            ],
                        );
                    }
                }
            }
            let db = db.lock().unwrap();
            db.execute(
                "INSERT INTO log_hits VALUES (?1,?2,?3)",
                rusqlite::params![rule.id, ts, count as i64],
            )
            .map_err(|_| "Log storage failed")?;
            let window = (p.window_secs.max(1).min(i64::MAX as u64 / 1000) * 1000) as i64;
            db.execute(
                "DELETE FROM log_hits WHERE rule=?1 AND ts<?2",
                rusqlite::params![rule.id, ts.saturating_sub(window)],
            )
            .map_err(|_| "Log pruning failed")?;
            let count = db
                .query_row(
                    "SELECT COALESCE(SUM(count),0) FROM log_hits WHERE rule=?1 AND ts>=?2",
                    rusqlite::params![rule.id, ts.saturating_sub(window)],
                    |r| r.get::<_, i64>(0),
                )
                .map_err(|_| "Log query failed")?;
            Ok(tracelyn_core::logic::log_rate_per_min(
                count.max(0) as u64,
                p.window_secs,
            ))
        }
        "metric" | "error_rate" | "p95" => {
            let key = tracelyn_core::types::default_metric_key(&rule.kind, &p.metric_key);
            let db = db.lock().unwrap();
            db.query_row(
                "SELECT value FROM raw WHERE metric=?1 AND ts>=?2 ORDER BY ts DESC LIMIT 1",
                rusqlite::params![key, ts - 15000],
                |r| r.get(0),
            )
            .map_err(|_| "Metric unavailable".into())
        }
        _ => Err("Rule requires desktop collectors".into()),
    }
}
fn ssl_days(host: &str, port: u16, ts: i64) -> Result<f64, String> {
    use std::net::ToSocketAddrs;
    let addr = (host, if port == 0 { 443 } else { port })
        .to_socket_addrs()
        .map_err(|_| "DNS failed")?
        .next()
        .ok_or("No address")?;
    let tcp = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(5))
        .map_err(|_| "TLS connect failed")?;
    tcp.set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|_| "TLS timeout")?;
    tcp.set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|_| "TLS timeout")?;
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name =
        rustls::pki_types::ServerName::try_from(host.to_owned()).map_err(|_| "Invalid TLS host")?;
    let conn =
        rustls::ClientConnection::new(Arc::new(config), name).map_err(|_| "TLS config failed")?;
    let mut stream = rustls::StreamOwned::new(conn, tcp);
    while stream.conn.is_handshaking() {
        stream
            .conn
            .complete_io(&mut stream.sock)
            .map_err(|_| "TLS handshake failed")?;
    }
    let cert = stream
        .conn
        .peer_certificates()
        .and_then(|c| c.first())
        .ok_or("No TLS certificate")?;
    let (_, cert) =
        x509_parser::parse_x509_certificate(cert.as_ref()).map_err(|_| "Invalid certificate")?;
    Ok((cert.validity().not_after.timestamp() - ts / 1000) as f64 / 86400.)
}

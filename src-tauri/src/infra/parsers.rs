//! Parsers accept bounded, read-only command output. Missing values stay null/absent.
use super::model::Resource;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn json_value(text: &str) -> Result<Value, String> {
    serde_json::from_str(text.trim())
        .map_err(|_| "The command returned an unsupported or incomplete JSON response.".into())
}
fn number(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str()?.parse().ok())
        .or_else(|| v.get("$numberLong")?.as_str()?.parse().ok())
}
fn put(row: &mut Resource, key: &str, value: &Value) {
    if let Some(n) = number(value) {
        row.set(key, n);
    } else if !value.is_null() {
        row.set(key, value.clone());
    }
}
pub fn database(engine: &str, text: &str) -> Result<Vec<Resource>, String> {
    let mut row = Resource::new(engine, "ok");
    if engine == "mysql" {
        let mut status = BTreeMap::new();
        let mut sizes = vec![];
        let mut in_sizes = false;
        for line in text.lines() {
            if line == "__SIZES__" {
                in_sizes = true;
                continue;
            }
            if line == "__REPLICATION__" {
                in_sizes = false;
                continue;
            }
            let cols: Vec<_> = line.split('\t').collect();
            if in_sizes && cols.len() == 2 {
                if let Ok(bytes) = cols[1].parse::<f64>() {
                    sizes.push(json!({"database":cols[0],"bytes":bytes}));
                }
            } else if cols.len() == 2 {
                status.insert(cols[0].trim(), cols[1].trim());
            }
            if let Some((key, value)) = line.trim().split_once(':') {
                if key == "Seconds_Behind_Master" || key == "Seconds_Behind_Source" {
                    if let Ok(n) = value.trim().parse::<f64>() {
                        row.set("replicationLagSeconds", n);
                    }
                }
            }
        }
        for (key, source) in [
            ("connections", "Threads_connected"),
            ("queryCounter", "Queries"),
            ("slowQueries", "Slow_queries"),
            ("uptimeSeconds", "Uptime"),
        ] {
            let n = status.get(source).and_then(|s| s.parse::<f64>().ok());
            if let Some(n) = n {
                row.set(key, n);
            }
        }
        let reads = status
            .get("Innodb_buffer_pool_reads")
            .and_then(|s| s.parse::<f64>().ok());
        let requests = status
            .get("Innodb_buffer_pool_read_requests")
            .and_then(|s| s.parse::<f64>().ok());
        if let (Some(reads), Some(req)) = (reads, requests) {
            if req > 0. {
                row.set(
                    "cacheHitPercent",
                    (100. * (1. - reads / req)).clamp(0., 100.),
                );
            }
        }
        row.set("sizes", Value::Array(sizes));
        row.set(
            "slowQueryScope",
            "Cumulative statements above the server long_query_time threshold.",
        );
    } else {
        let v = json_value(text)?;
        if engine == "postgres" {
            for key in [
                "connections",
                "queryCounter",
                "slowQueries",
                "cacheHitPercent",
                "replicationLagSeconds",
                "sizes",
                "counterEpoch",
            ] {
                put(&mut row, key, &v[key]);
            }
            row.set("rateLabel", "Transactions/s");
            row.set("slowQueryScope","Currently active queries running longer than 1 second. QPS column is committed plus rolled-back transactions/s.");
        } else {
            let s = &v["status"];
            put(&mut row, "connections", &s["connections"]["current"]);
            if let Some(ops) = s["opcounters"].as_object() {
                row.set("queryCounter", ops.values().filter_map(number).sum::<f64>());
            }
            put(&mut row, "uptimeSeconds", &s["uptime"]);
            put(&mut row, "slowQueries", &v["slowQueries"]);
            put(&mut row, "sizes", &v["sizes"]);
            let cache = &s["wiredTiger"]["cache"];
            if let (Some(read), Some(req)) = (
                number(&cache["pages read into cache"]),
                number(&cache["pages requested from the cache"]),
            ) {
                if req > 0. {
                    row.set(
                        "cacheHitPercent",
                        (100. * (1. - read / req)).clamp(0., 100.),
                    );
                }
            }
            if let Some(members) = v["replication"]["members"].as_array() {
                let latest = members
                    .iter()
                    .filter_map(|m| number(&m["optimeDateMs"]))
                    .fold(0., f64::max);
                let lag = members
                    .iter()
                    .filter(|m| m["self"].as_bool() == Some(true))
                    .filter_map(|m| number(&m["optimeDateMs"]))
                    .next();
                if let Some(own) = lag {
                    row.set("replicationLagSeconds", ((latest - own) / 1000.).max(0.));
                }
            }
            row.set("rateLabel", "Operations/s");
            row.set("slowQueryScope","Captured system.profile entries at least 100 ms in the selected database. Unavailable when profiling is disabled. Cache ratio estimates WiredTiger page hits.");
            if let Some(warnings) = v["warnings"].as_array() {
                row.set("limitations", Value::Array(warnings.clone()));
            }
        }
    }
    if row.number("connections").is_none() {
        return Err("No database status metrics were returned. Check authentication and monitoring permissions.".into());
    }
    Ok(vec![row])
}

pub fn rabbitmq(text: &str) -> Result<Vec<Resource>, String> {
    if text.contains("__VHOST__\t") {
        let mut rows = vec![];
        for block in text.split("__VHOST__\t").skip(1) {
            let (vhost, body) = block.split_once('\n').ok_or("Invalid vhost output")?;
            let mut queues = json_value(body)?;
            for q in queues.as_array_mut().ok_or("Invalid queue array")? {
                q["vhost"] = serde_json::json!(vhost);
            }
            rows.extend(rabbitmq(&queues.to_string())?);
        }
        rows.truncate(500);
        return Ok(rows);
    }
    let v = json_value(text)?;
    let list = v
        .as_array()
        .or_else(|| v["items"].as_array())
        .ok_or("RabbitMQ returned no queue array")?;
    Ok(list
        .iter()
        .take(500)
        .map(|q| {
            let name = format!(
                "{} / {}",
                q["vhost"].as_str().unwrap_or("/"),
                q["name"].as_str().unwrap_or("unknown")
            );
            let mut row = Resource::new(name, q["state"].as_str().unwrap_or("unknown"));
            for (key, path) in [
                ("depth", "messages"),
                ("ready", "messages_ready"),
                ("unacknowledged", "messages_unacknowledged"),
                ("consumers", "consumers"),
            ] {
                put(&mut row, key, &q[path]);
            }
            put(
                &mut row,
                "publishPerSecond",
                &q["message_stats"]["publish_details"]["rate"],
            );
            put(
                &mut row,
                "deliverPerSecond",
                &q["message_stats"]["deliver_get_details"]["rate"],
            );
            row
        })
        .collect())
}
pub fn kafka(text: &str) -> Vec<Resource> {
    let mut rows = vec![];
    let mut header: Vec<&str> = vec![];
    for line in text.lines() {
        let cols: Vec<_> = line.split_whitespace().collect();
        if cols.contains(&"CURRENT-OFFSET") && cols.contains(&"LAG") {
            header = cols;
            continue;
        }
        if line.contains("PartitionCount:") {
            let mut row = Resource::new(
                cols.windows(2)
                    .find(|c| c[0] == "Topic:")
                    .map(|c| c[1])
                    .unwrap_or("topic"),
                "topic",
            );
            for (key, label) in [
                ("partitions", "PartitionCount:"),
                ("replicationFactor", "ReplicationFactor:"),
            ] {
                if let Some(n) = cols
                    .windows(2)
                    .find(|c| c[0] == label)
                    .and_then(|c| c[1].parse::<u64>().ok())
                {
                    row.set(key, n);
                }
            }
            rows.push(row);
            continue;
        }
        if header.is_empty() || cols.len() < header.len().saturating_sub(1) {
            continue;
        }
        let get = |name: &str| {
            header
                .iter()
                .position(|s| *s == name)
                .and_then(|i| cols.get(i))
                .copied()
                .unwrap_or("-")
        };
        if get("PARTITION").parse::<u64>().is_err() {
            continue;
        }
        let lag = get("LAG").parse::<u64>().ok();
        let mut row = Resource::new(
            format!("{} / {} / {}", get("GROUP"), get("TOPIC"), get("PARTITION")),
            if lag.unwrap_or(0) > 0 {
                "warning"
            } else {
                "ok"
            },
        );
        for (key, label) in [
            ("lag", "LAG"),
            ("currentOffset", "CURRENT-OFFSET"),
            ("logEndOffset", "LOG-END-OFFSET"),
        ] {
            if let Ok(n) = get(label).parse::<u64>() {
                row.set(key, n);
            }
        }
        row.set("consumer", get("CONSUMER-ID"));
        rows.push(row);
    }
    rows.truncate(500);
    rows
}
fn csv_line(line: &str) -> Vec<String> {
    let mut cells = vec![];
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                if quoted && chars.peek() == Some(&'"') {
                    cell.push('"');
                    chars.next();
                } else {
                    quoted = !quoted;
                }
            }
            ',' if !quoted => {
                cells.push(std::mem::take(&mut cell));
            }
            _ => cell.push(c),
        }
    }
    cells.push(cell);
    cells
}
pub fn haproxy(text: &str) -> Vec<Resource> {
    let mut lines = text.lines();
    let header = csv_line(lines.next().unwrap_or("").trim_start_matches('#').trim());
    lines
        .filter_map(|line| {
            let cols = csv_line(line);
            let get = |key: &str| {
                header
                    .iter()
                    .position(|h| h == key)
                    .and_then(|i| cols.get(i))
                    .map(String::as_str)
                    .unwrap_or("")
            };
            if get("svname").is_empty() || ["FRONTEND", "BACKEND"].contains(&get("svname")) {
                return None;
            }
            let state = if get("status").starts_with("UP") {
                "up"
            } else if get("status").starts_with("DOWN") {
                "down"
            } else {
                "unknown"
            };
            let mut row = Resource::new(format!("{} / {}", get("pxname"), get("svname")), state);
            row.set("health", get("status"));
            for (key, label) in [
                ("weight", "weight"),
                ("connections", "scur"),
                ("rate", "rate"),
                ("requestsTotal", "stot"),
                ("errorsTotal", "eresp"),
            ] {
                if let Ok(n) = get(label).parse::<f64>() {
                    row.set(key, n);
                }
            }
            if let (Some(errors), Some(total)) =
                (row.number("errorsTotal"), row.number("requestsTotal"))
            {
                if total > 0. {
                    row.set("errorPercent", errors / total * 100.);
                }
            }
            Some(row)
        })
        .take(500)
        .collect()
}
pub fn proxy_api(kind: &str, text: &str) -> Result<Vec<Resource>, String> {
    let v = json_value(text)?;
    let list = v.as_array().ok_or("Proxy API returned no array")?;
    let mut rows = vec![];
    for item in list.iter().take(500) {
        if kind == "traefik" {
            let name = item["name"].as_str().unwrap_or("service");
            if let Some(statuses) = item["serverStatus"].as_object() {
                for (address, status) in statuses {
                    let mut row = Resource::new(
                        format!("{name} / {address}"),
                        if status.as_str() == Some("UP") {
                            "up"
                        } else {
                            "down"
                        },
                    );
                    row.set("address", address.clone());
                    if let Some(servers) = item["loadBalancer"]["servers"].as_array() {
                        if let Some(server) =
                            servers.iter().find(|s| s["url"].as_str() == Some(address))
                        {
                            put(&mut row, "weight", &server["weight"]);
                        }
                    }
                    row.set("note","Error rates require access-log or metrics configuration; the configuration API does not report them.");
                    rows.push(row);
                }
            } else {
                let mut row = Resource::new(name, item["status"].as_str().unwrap_or("unknown"));
                row.set("note", "No per-backend health reported by this service.");
                rows.push(row);
            }
        } else {
            let mut row = Resource::new(item["address"].as_str().unwrap_or("upstream"), "unknown");
            put(&mut row, "activeRequests", &item["num_requests"]);
            put(&mut row, "failures", &item["fails"]);
            row.set("note","Passive failures and active requests are gauges. They do not establish backend health or an error rate; TCP probe is shown separately.");
            rows.push(row);
        }
    }
    Ok(rows)
}

pub fn docker(text: &str) -> Result<Vec<Resource>, String> {
    let (inspect, stats) = text.split_once("__STATS__").unwrap_or((text, ""));
    let v = json_value(inspect)?;
    let list = v.as_array().ok_or("Docker inspect returned no array")?;
    let stats: Vec<Value> = stats
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    Ok(list
        .iter()
        .take(500)
        .map(|c| {
            let name = c["Name"]
                .as_str()
                .unwrap_or("container")
                .trim_start_matches('/');
            let state = c["State"]["Status"].as_str().unwrap_or("unknown");
            let mut row = Resource::new(name, state);
            row.id = c["Id"].as_str().unwrap_or(name).into();
            put(&mut row, "restarts", &c["RestartCount"]);
            put(&mut row, "health", &c["State"]["Health"]["Status"]);
            put(
                &mut row,
                "composeProject",
                &c["Config"]["Labels"]["com.docker.compose.project"],
            );
            put(&mut row, "image", &c["Config"]["Image"]);
            if let Some(ports) = c["NetworkSettings"]["Ports"].as_object() {
                let mut items = vec![];
                for (port, bindings) in ports {
                    if let Some(bindings) = bindings.as_array() {
                        for b in bindings {
                            items.push(format!(
                                "{}:{} → {port}",
                                b["HostIp"].as_str().unwrap_or(""),
                                b["HostPort"].as_str().unwrap_or("")
                            ));
                        }
                    }
                }
                row.set("ports", items.join(", "));
            }
            if let Some(s) = stats.iter().find(|s| {
                s["Name"].as_str() == Some(name)
                    || s["ID"]
                        .as_str()
                        .map(|id| row.id.starts_with(id))
                        .unwrap_or(false)
            }) {
                if let Some(cpu) = s["CPUPerc"]
                    .as_str()
                    .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok())
                {
                    row.set("cpuPercent", cpu);
                }
                put(&mut row, "memory", &s["MemUsage"]);
                if let Some(memory) = s["MemPerc"]
                    .as_str()
                    .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok())
                {
                    row.set("memoryPercent", memory);
                }
            }
            row
        })
        .collect())
}
pub fn kubernetes(text: &str) -> Result<Vec<Resource>, String> {
    let v = json_value(text)?;
    let items = v["items"].as_array().ok_or("kubectl returned no items")?;
    Ok(items
        .iter()
        .take(500)
        .map(|item| {
            let kind = item["kind"].as_str().unwrap_or("Pod");
            let name = format!(
                "{} / {} / {}",
                kind,
                item["metadata"]["namespace"].as_str().unwrap_or("cluster"),
                item["metadata"]["name"].as_str().unwrap_or("unknown")
            );
            let statuses = item["status"]["containerStatuses"].as_array();
            let (state, restarts) = if kind == "Node" {
                let ready = item["status"]["conditions"]
                    .as_array()
                    .and_then(|l| l.iter().find(|c| c["type"] == "Ready"))
                    .and_then(|c| c["status"].as_str());
                (
                    if ready == Some("True") {
                        "ready"
                    } else {
                        "not_ready"
                    },
                    0,
                )
            } else {
                (
                    item["status"]["phase"].as_str().unwrap_or("unknown"),
                    statuses
                        .map(|l| l.iter().filter_map(|s| s["restartCount"].as_u64()).sum())
                        .unwrap_or(0),
                )
            };
            let mut row = Resource::new(name, state);
            row.set("kind", kind);
            row.set("restarts", restarts);
            put(&mut row, "node", &item["spec"]["nodeName"]);
            let ready = statuses
                .map(|l| {
                    l.iter()
                        .filter(|s| s["ready"].as_bool() == Some(true))
                        .count()
                })
                .unwrap_or(0);
            row.set("readyContainers", ready);
            row.set("reason", item["status"]["reason"].as_str().unwrap_or(""));
            row
        })
        .collect())
}

pub fn domains(text: &str) -> Vec<String> {
    let re = Regex::new(r"(?im)(?:^|[;{}])\s*(?:server_name|ServerName|ServerAlias)\s+([^;\n}]+)")
        .unwrap();
    let mut names = BTreeSet::new();
    for cap in re.captures_iter(text) {
        for token in cap[1].split('#').next().unwrap_or("").split_whitespace() {
            if let Some(name) = valid_domain(token) {
                names.insert(name);
            }
        }
    }
    names.into_iter().take(2048).collect()
}
pub fn valid_domain(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('.').to_ascii_lowercase();
    if value.len() > 253 || !value.contains('.') || value.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    if value.split('.').all(|s| {
        !s.is_empty()
            && s.len() <= 63
            && !s.starts_with('-')
            && !s.ends_with('-')
            && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    }) {
        Some(value)
    } else {
        None
    }
}
pub fn certificates(text: &str, now: i64) -> Vec<Resource> {
    let mut rows = vec![];
    for block in text.split("__CERT__\t").skip(1) {
        let mut lines = block.lines();
        let path = lines.next().unwrap_or("");
        let mut row = Resource::new(path, "unknown");
        for line in lines {
            if let Some(date) = line.strip_prefix("notAfter=") {
                if let Ok(date) =
                    chrono::NaiveDateTime::parse_from_str(date.trim(), "%b %e %H:%M:%S %Y GMT")
                {
                    let expiry = date.and_utc().timestamp_millis();
                    certificate_state(&mut row, expiry, now);
                }
            } else if let Some(subject) = line.strip_prefix("subject=") {
                row.set("subject", subject);
            }
        }
        if row.state == "unknown" {
            row.set("note","Cannot read certificate or parse its expiry. Check file access and openssl availability.");
        }
        rows.push(row);
    }
    rows
}
pub fn certificate_state(row: &mut Resource, expiry: i64, now: i64) {
    let days = (expiry - now) as f64 / 86_400_000.;
    row.set("expiresAtMs", expiry);
    row.set("daysRemaining", days);
    row.state = if days < 7. {
        "critical"
    } else if days < 21. {
        "warning"
    } else {
        "ok"
    }
    .into();
}
pub fn disks(text: &str) -> Vec<Resource> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let c: Vec<_> = line.split_whitespace().collect();
            if c.len() < 6 {
                return None;
            }
            let total = c[1].parse::<f64>().ok()? * 1024.;
            let used = c[2].parse::<f64>().ok()? * 1024.;
            if total <= 0. {
                return None;
            }
            let mount = c[5..].join(" ").replace("\\040", " ");
            let percent = c[4].trim_end_matches('%').parse::<f64>().ok()?;
            let mut row = Resource::new(
                mount,
                if percent >= 95. {
                    "critical"
                } else if percent >= 85. {
                    "warning"
                } else {
                    "ok"
                },
            );
            row.set("totalBytes", total);
            row.set("usedBytes", used);
            row.set("usedPercent", percent);
            Some(row)
        })
        .take(256)
        .collect()
}
pub fn ports(text: &str) -> Vec<Resource> {
    let mut grouped: Vec<PortGroup> = Vec::new();
    for line in text.lines() {
        let Some(row) = parse_port_line(line) else {
            continue;
        };
        if let Some(existing) = grouped.iter_mut().find(|g| {
            g.proto == row.proto && g.port == row.port && g.process.eq_ignore_ascii_case(&row.process)
        }) {
            if !existing.addresses.iter().any(|a| a == &row.address) {
                existing.addresses.push(row.address);
            }
            existing.exposed |= row.exposed;
            if existing.process.is_empty() {
                existing.process = row.process;
            }
        } else {
            grouped.push(PortGroup {
                proto: row.proto,
                port: row.port,
                process: row.process,
                addresses: vec![row.address],
                exposed: row.exposed,
            });
        }
        if grouped.len() >= 500 {
            break;
        }
    }
    grouped
        .into_iter()
        .map(|g| {
            let label = if g.process.is_empty() {
                format!("{} :{}", g.proto, g.port)
            } else {
                format!("{} {}:{}", g.proto, g.process, g.port)
            };
            let mut row = Resource::new(&label, if g.exposed { "exposed" } else { "local" });
            row.set("address", g.addresses.join(", "));
            row.set("process", g.process.clone());
            if g.process.is_empty() {
                row.set(
                    "note",
                    "Process ownership is hidden; elevated read permission may be required.",
                );
            }
            row
        })
        .collect()
}

struct PortGroup {
    proto: String,
    port: u16,
    process: String,
    addresses: Vec<String>,
    exposed: bool,
}

struct ParsedPort {
    proto: String,
    port: u16,
    process: String,
    address: String,
    exposed: bool,
}

fn parse_port_line(line: &str) -> Option<ParsedPort> {
    let c: Vec<_> = line.split_whitespace().collect();
    let index = if c.first() == Some(&"tcp") || c.first() == Some(&"udp") {
        4
    } else {
        3
    };
    let address = (*c.get(index)?).to_string();
    if !address.contains(':') {
        return None;
    }
    let port = address
        .rsplit_once(':')?
        .1
        .trim_matches(|ch| ch == ']' || ch == '[')
        .parse()
        .ok()?;
    let proto = if index == 4 { c[0].to_string() } else { "tcp".into() };
    let rest = c.get(index + 2..).unwrap_or(&[]).join(" ");
    let process = rest
        .split("((\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .unwrap_or("")
        .to_string();
    let exposed =
        address.starts_with("0.0.0.0:") || address.starts_with("[::]:") || address.starts_with("*:");
    Some(ParsedPort {
        proto,
        port,
        process,
        address,
        exposed,
    })
}
pub fn ssh_failures(text: &str) -> Vec<Resource> {
    let ip = Regex::new(r"(?:from|rhost=)\s*([0-9a-fA-F:.]+)").unwrap();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut timeline: BTreeMap<i64, u64> = BTreeMap::new();
    let offset = text
        .lines()
        .find_map(|l| l.strip_prefix("__OFFSET__\t"))
        .unwrap_or("+0000");
    for line in text.lines().filter(|l| {
        l.contains("Failed password")
            || l.contains("Invalid user")
            || l.contains("authentication failure")
    }) {
        if let Some(c) = ip.captures(line) {
            if c[1].parse::<std::net::IpAddr>().is_ok() {
                *counts.entry(c[1].into()).or_default() += 1;
            }
        }
        let token = line.split_whitespace().next().unwrap_or("");
        let timestamp = chrono::DateTime::parse_from_rfc3339(token)
            .or_else(|_| chrono::DateTime::parse_from_str(token, "%Y-%m-%dT%H:%M:%S%z"))
            .ok()
            .map(|t| t.timestamp_millis())
            .or_else(|| {
                use chrono::Datelike;
                let now = chrono::Utc::now();
                let year = now.year();
                let prefix = line.get(..15)?;
                let parse = |year| {
                    chrono::DateTime::parse_from_str(
                        &format!("{year} {prefix} {offset}"),
                        "%Y %b %e %H:%M:%S %z",
                    )
                    .ok()
                    .map(|t| t.timestamp_millis())
                };
                let ts = parse(year)?;
                if ts > now.timestamp_millis() + 86_400_000 {
                    parse(year - 1)
                } else {
                    Some(ts)
                }
            });
        if let Some(timestamp) = timestamp {
            *timeline
                .entry(timestamp / 3_600_000 * 3_600_000)
                .or_default() += 1;
        }
    }
    let timeline: Vec<_> = timeline
        .into_iter()
        .map(|(hour, count)| json!({"tsMs":hour,"count":count}))
        .collect();
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    counts
        .into_iter()
        .take(100)
        .map(|(address, count)| {
            let mut row = Resource::new(address, "warning");
            row.set("attempts", count);
            row.set("timeline", json!(timeline));
            row
        })
        .collect()
}
pub fn updates(text: &str) -> Vec<Resource> {
    let mut rows = vec![];
    let mut security = false;
    for line in text.lines() {
        if line == "__SECURITY__" {
            security = true;
            continue;
        }
        let c: Vec<_> = line.split_whitespace().collect();
        if c.len() < 2 {
            continue;
        }
        if line.contains("[upgradable from:")
            || (c.len() >= 3
                && c[0].contains('.')
                && !line.starts_with("Last ")
                && !line.starts_with("Loaded "))
        {
            let mut row = Resource::new(
                c[0],
                if security || line.to_ascii_lowercase().contains("security") {
                    "security"
                } else {
                    "update"
                },
            );
            row.set("version", c[1]);
            row.set("source", c.get(2).copied().unwrap_or(""));
            rows.push(row);
        }
    }
    let mut by_name: BTreeMap<String, Resource> = BTreeMap::new();
    for row in rows {
        let name = row.name.split('/').next().unwrap_or(&row.name).to_string();
        if !by_name.contains_key(&name) || row.state == "security" {
            by_name.insert(name, row);
        }
    }
    by_name.into_values().take(1000).collect()
}
pub fn fail2ban(text: &str) -> Vec<Resource> {
    text.split("__JAIL__\t")
        .skip(1)
        .map(|block| {
            let mut lines = block.lines();
            let mut row = Resource::new(lines.next().unwrap_or("jail"), "ok");
            for line in lines {
                if let Some((key, value)) = line.split_once(':') {
                    if key.contains("Currently banned") {
                        if let Ok(n) = value.trim().parse::<u64>() {
                            row.set("banned", n);
                        }
                    } else if key.contains("Banned IP list") {
                        row.set("bannedIps", value.trim());
                    } else if key.contains("Total failed") {
                        if let Ok(n) = value.trim().parse::<u64>() {
                            row.set("failed", n);
                        }
                    }
                }
            }
            row
        })
        .take(100)
        .collect()
}
pub fn last_logins(text: &str) -> Vec<Resource> {
    text.lines()
        .filter(|l| {
            !l.trim().is_empty()
                && !l.starts_with("wtmp begins")
                && !l.starts_with("reboot ")
                && !l.starts_with("shutdown ")
        })
        .take(100)
        .enumerate()
        .map(|(i, line)| {
            let mut row = Resource::new(
                line.split_whitespace().next().unwrap_or("login"),
                "recorded",
            );
            row.id = format!("login-{i}");
            row.set("session", line);
            row
        })
        .collect()
}

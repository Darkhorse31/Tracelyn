use super::model::Resource;
use regex::Regex;

pub fn upstreams(config: &str) -> Vec<Resource> {
    let clean = config
        .lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let blocks = Regex::new(r"(?s)upstream\s+([^\s{]+)\s*\{([^}]+)\}").unwrap();
    let server = Regex::new(r"\bserver\s+([^\s;]+)([^;]*);").unwrap();
    let mut rows = vec![];
    for block in blocks.captures_iter(&clean) {
        for cap in server.captures_iter(&block[2]) {
            let options: Vec<_> = cap[2].split_whitespace().collect();
            let disabled = options.contains(&"down");
            let mut row = Resource::new(
                format!("{} / {}", &block[1], &cap[1]),
                if disabled { "down" } else { "unknown" },
            );
            row.set("address", cap[1].to_string());
            row.set("configuredDown", disabled);
            row.set(
                "weight",
                options
                    .iter()
                    .find_map(|s| s.strip_prefix("weight=")?.parse::<u64>().ok())
                    .unwrap_or(1),
            );
            row.set("backup", options.contains(&"backup"));
            rows.push(row);
        }
    }
    // Direct proxy_pass destinations do not belong to a named upstream block.
    let direct = Regex::new(r"proxy_pass\s+https?://([^\s/;$]+)").unwrap();
    for cap in direct.captures_iter(&clean) {
        let address = &cap[1];
        if address.contains(':') && !rows.iter().any(|r| r.fields["address"] == address) {
            let mut row = Resource::new(format!("direct / {address}"), "unknown");
            row.set("address", address);
            row.set("weight", 1);
            rows.push(row);
        }
    }
    rows.truncate(500);
    rows
}
pub fn log_paths(config: &str) -> Vec<String> {
    let re = Regex::new(r"(?m)\b(?:access_log|error_log)\s+(/[^;\s]+)").unwrap();
    let mut paths: Vec<_> = re.captures_iter(config).map(|c| c[1].into()).collect();
    paths.sort();
    paths.dedup();
    paths.truncate(24);
    paths
}
pub fn add_log_health(rows: &mut [Resource], logs: &str) {
    let combined =
        Regex::new(r#""(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|CONNECT) [^\"]*"\s+(\d{3})\s"#)
            .unwrap();
    for row in rows {
        let address = row.fields["address"].as_str().unwrap_or("");
        let mut requests = 0;
        let mut errors = 0;
        let mut failed_connections = 0;
        for line in logs.lines().filter(|l| l.contains(address)) {
            if let Some(cap) = combined.captures(line) {
                requests += 1;
                if cap[1].starts_with('5') {
                    errors += 1;
                }
            }
            if line.contains("connect() failed")
                || line.contains("upstream timed out")
                || line.contains("no live upstreams")
            {
                failed_connections += 1;
            }
        }
        row.set("recentConnectionErrors", failed_connections);
        if requests > 0 {
            row.set("recentRequests", requests);
            row.set("errorPercent", 100. * errors as f64 / requests as f64);
        }
        row.set("note","TCP reachability is a point-in-time probe. Log error rate uses only sampled requests containing this upstream address. Absent means the log cannot attribute requests.");
    }
}

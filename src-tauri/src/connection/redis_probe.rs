use crate::collectors::redis::{connect_host, Dial};
use crate::ssh::{run_command, SshSession};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisEndpoint {
    pub label: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub socket: Option<String>,
    pub docker_container: Option<String>,
    pub bind: Option<String>,
    #[serde(default)]
    pub tls: bool,
}

impl RedisEndpoint {
    pub fn to_dial(&self) -> Dial {
        let host = connect_host(self.host.as_deref());
        let port = self.port.unwrap_or(0);
        Dial {
            host: host.clone(),
            port,
            socket: self.socket.clone(),
            docker: self.docker_container.clone(),
            tls: self.tls,
            bind_label: self
                .bind
                .clone()
                .unwrap_or_else(|| format!("{host}:{port}")),
        }
    }
}

pub async fn probe_redis_endpoints(session: &SshSession) -> Vec<RedisEndpoint> {
    let script = r#"
echo '---ss---'
ss -ltnpH 2>/dev/null | grep -i redis || true
echo '---sock---'
ls /var/run/redis/*.sock /run/redis/*.sock /tmp/redis.sock 2>/dev/null || true
echo '---docker---'
docker ps --format '{{.Names}} {{.Image}}' 2>/dev/null | grep -i redis || true
echo '---cli---'
command -v redis-cli || true
echo '---conf---'
for f in /etc/redis/redis.conf /etc/redis.conf /usr/local/etc/redis.conf; do
  if [ -r "$f" ]; then
    grep -E '^[[:space:]]*(bind|port|unixsocket|tls-port)[[:space:]]' "$f"
  fi
done
"#;
    let out = run_command(session, script).await.unwrap_or_default();
    parse_redis_probe(&out)
}

pub fn parse_redis_probe(text: &str) -> Vec<RedisEndpoint> {
    let mut section = "";
    let mut endpoints = Vec::new();
    let mut saw_cli = false;
    let mut conf_bind: Vec<String> = Vec::new();
    let mut conf_port: Option<u16> = None;
    let mut conf_socket: Option<String> = None;
    let mut tls_port: Option<u16> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("---") {
            section = line;
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match section {
            "---ss---" => {
                if let Some((host, port)) = listen_host_port(line) {
                    endpoints.push(tcp_endpoint(host, port));
                }
            }
            "---sock---" => {
                endpoints.push(RedisEndpoint {
                    label: format!("socket:{line}"),
                    host: None,
                    port: None,
                    socket: Some(line.to_string()),
                    docker_container: None,
                    bind: Some(line.to_string()),
                    tls: false,
                });
            }
            "---docker---" => {
                let name = line.split_whitespace().next().unwrap_or(line);
                let name = name.trim_start_matches('/');
                endpoints.push(RedisEndpoint {
                    label: format!("docker:{name}"),
                    host: Some("127.0.0.1".into()),
                    port: Some(6379),
                    socket: None,
                    docker_container: Some(name.to_string()),
                    bind: None,
                    tls: false,
                });
            }
            "---cli---" => saw_cli = true,
            "---conf---" => {
                let mut parts = line.split_whitespace();
                let key = parts.next().unwrap_or("");
                match key {
                    "bind" => {
                        conf_bind.extend(parts.map(str::to_string));
                    }
                    "port" => conf_port = parts.next().and_then(|p| p.parse().ok()),
                    "unixsocket" => conf_socket = parts.next().map(str::to_string),
                    "tls-port" => tls_port = parts.next().and_then(|p| p.parse().ok()),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if let Some(socket) = conf_socket {
        if !endpoints.iter().any(|e| e.socket.as_deref() == Some(socket.as_str())) {
            endpoints.push(RedisEndpoint {
                label: format!("socket:{socket}"),
                host: None,
                port: None,
                socket: Some(socket.clone()),
                docker_container: None,
                bind: Some(socket),
                tls: false,
            });
        }
    }
    let port = conf_port.unwrap_or(6379);
    let tls_only = tls_port.unwrap_or(0) > 0 && conf_port == Some(0);
    let bind_hosts: Vec<String> = conf_bind
        .iter()
        .filter(|h| !h.starts_with('-'))
        .cloned()
        .collect();
    let preferred = bind_hosts
        .iter()
        .find(|h| h.as_str() == "127.0.0.1")
        .cloned()
        .or_else(|| bind_hosts.iter().find(|h| !h.contains(':')).cloned());
    if let Some(host) = preferred {
        let dial_host = connect_host(Some(&host));
        if let Some(existing) = endpoints.iter_mut().find(|e| e.port == Some(port) && e.docker_container.is_none()) {
            existing.host = Some(dial_host);
            existing.bind = Some(bind_hosts.join(" "));
            existing.tls = tls_only;
        } else if !bind_hosts.is_empty() {
            let mut ep = tcp_endpoint(dial_host, port);
            ep.bind = Some(bind_hosts.join(" "));
            ep.tls = tls_only;
            endpoints.push(ep);
        }
    }
    if endpoints.is_empty() && saw_cli {
        endpoints.push(tcp_endpoint("127.0.0.1".into(), 6379));
    }
    // One TCP target per port. Prefer 127.0.0.1 over ::1.
    let mut best: Vec<RedisEndpoint> = Vec::new();
    for ep in endpoints {
        if ep.port.is_some() && ep.docker_container.is_none() && ep.socket.is_none() {
            if let Some(prev) = best.iter_mut().find(|p| p.port == ep.port && p.docker_container.is_none() && p.socket.is_none()) {
                if ep.host.as_deref() == Some("127.0.0.1") {
                    *prev = ep;
                }
                continue;
            }
        }
        best.push(ep);
    }
    best.sort_by_key(|e| {
        (
            if e.port.is_some() && e.docker_container.is_none() { 0 } else if e.socket.is_some() { 1 } else { 2 },
            e.label.clone(),
        )
    });
    best.dedup_by(|a, b| a.label == b.label);
    best
}

fn tcp_endpoint(host: String, port: u16) -> RedisEndpoint {
    RedisEndpoint {
        label: format!("redis:{port}"),
        host: Some(if host == "*" || host == "0.0.0.0" { "127.0.0.1".into() } else { host.clone() }),
        port: Some(port),
        socket: None,
        docker_container: None,
        bind: Some(format!("{host}:{port}")),
        tls: false,
    }
}

fn listen_host_port(line: &str) -> Option<(String, u16)> {
    line.split_whitespace().find_map(split_host_port)
}

fn split_host_port(tok: &str) -> Option<(String, u16)> {
    if let Some(rest) = tok.strip_prefix('[') {
        let (host, port) = rest.split_once("]:")?;
        let port = port.parse().ok()?;
        return (port > 0).then_some((host.to_string(), port));
    }
    let (host, port) = tok.rsplit_once(':')?;
    let port = port.parse().ok()?;
    if port == 0 || host.is_empty() || host == "*" {
        return None;
    }
    Some((host.to_string(), port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_port_socket_and_docker() {
        let text = "\
---ss---
LISTEN 0 128 127.0.0.1:6380 0.0.0.0:* users:((\"redis-server\",pid=9,fd=6))
---sock---
/var/run/redis/redis.sock
---docker---
cache redis:7
---cli---
/usr/bin/redis-cli
";
        let eps = parse_redis_probe(text);
        assert!(eps.iter().any(|e| e.port == Some(6380)));
        assert!(eps.iter().any(|e| e.socket.as_deref() == Some("/var/run/redis/redis.sock")));
        assert!(eps.iter().any(|e| e.docker_container.as_deref() == Some("cache")));
        assert!(eps.iter().position(|e| e.port == Some(6380)).unwrap() < eps.iter().position(|e| e.docker_container.is_some()).unwrap());
    }

    #[test]
    fn prefers_ipv4_loopback_over_ipv6_and_reads_bind() {
        let text = "\
---ss---
LISTEN 0 511 127.0.0.1:6379 0.0.0.0:* users:((\"redis-server\",pid=1,fd=6))
LISTEN 0 511 [::1]:6379 [::]:* users:((\"redis-server\",pid=1,fd=7))
---conf---
bind 127.0.0.1 -::1
port 6379
";
        let eps = parse_redis_probe(text);
        let tcp: Vec<_> = eps.iter().filter(|e| e.port == Some(6379)).collect();
        assert_eq!(tcp.len(), 1);
        assert_eq!(tcp[0].host.as_deref(), Some("127.0.0.1"));
        assert!(tcp[0].bind.as_deref().unwrap_or("").contains("127.0.0.1"));
        assert!(!tcp[0].bind.as_deref().unwrap_or("").contains("-::1"));
    }
}

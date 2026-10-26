use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Listener {
    pub port: u16,
    pub pid: Option<u32>,
    pub process_name: String,
    pub addresses: Vec<String>,
}

pub fn parse_ss_ltnp(text: &str) -> Vec<Listener> {
    let mut listeners = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let address = line.split_whitespace().find(|tok| local_port(tok).is_some());
        let Some(address) = address else {
            continue;
        };
        let port = local_port(address).unwrap_or(0);
        if port == 0 {
            continue;
        }
        let (pid, name) = parse_users_process(line);
        listeners.push(Listener {
            port,
            pid,
            process_name: name,
            addresses: vec![address.to_string()],
        });
    }
    dedupe_listeners(listeners)
}

fn local_port(token: &str) -> Option<u16> {
    if !token.contains(':') {
        return None;
    }
    let port = token.rsplit_once(':')?.1.trim_matches(|c| c == ']' || c == '[');
    if port == "*" || port.is_empty() {
        return None;
    }
    let port: u16 = port.parse().ok()?;
    if port == 0 {
        return None;
    }
    Some(port)
}

/// One row per process + port. IPv4 and IPv6 listeners share that row and keep every address.
pub fn dedupe_listeners(listeners: Vec<Listener>) -> Vec<Listener> {
    let mut out: Vec<Listener> = Vec::new();
    for listener in listeners {
        if let Some(existing) = out.iter_mut().find(|row| {
            row.port == listener.port && row.process_name.eq_ignore_ascii_case(&listener.process_name)
        }) {
            for addr in listener.addresses {
                if !existing.addresses.iter().any(|have| have == &addr) {
                    existing.addresses.push(addr);
                }
            }
            if existing.pid.is_none() {
                existing.pid = listener.pid;
            }
        } else {
            out.push(listener);
        }
    }
    out
}

fn parse_users_process(line: &str) -> (Option<u32>, String) {
    if let Some(users) = line.split("users:(").nth(1) {
        let inner = users.trim_end_matches(')');
        let first = inner.split(',').next().unwrap_or(inner);
        let parts: Vec<&str> = first.split('"').collect();
        let name = parts.get(1).map(|s| s.to_string()).unwrap_or_else(|| "unknown".to_string());
        let pid = parts
            .get(2)
            .and_then(|s| s.trim_matches(&[',', ')', ' '][..]).parse().ok());
        return (pid, name);
    }
    (None, "unknown".to_string())
}

pub fn port_to_pid_map(listeners: &[Listener]) -> HashMap<u16, u32> {
    let mut m = HashMap::new();
    for l in listeners {
        if let Some(pid) = l.pid {
            m.insert(l.port, pid);
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ss_output() {
        let fixture = include_str!("../fixtures/ss_ltnp.txt");
        let listeners = parse_ss_ltnp(fixture);
        assert!(listeners.iter().any(|l| l.port == 3001));
    }

    #[test]
    fn merges_ipv4_and_ipv6_for_the_same_process_and_port() {
        let text = "\
LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=42,fd=3))
LISTEN 0 128 [::]:22 [::]:* users:((\"sshd\",pid=42,fd=4))
LISTEN 0 128 127.0.0.1:5432 0.0.0.0:* users:((\"postgres\",pid=7,fd=5))
LISTEN 0 128 [::1]:5432 [::]:* users:((\"postgres\",pid=7,fd=6))
";
        let listeners = parse_ss_ltnp(text);
        assert_eq!(listeners.len(), 2);
        let ssh = listeners.iter().find(|l| l.port == 22).unwrap();
        assert_eq!(ssh.process_name, "sshd");
        assert_eq!(ssh.addresses, vec!["0.0.0.0:22".to_string(), "[::]:22".to_string()]);
        let pg = listeners.iter().find(|l| l.port == 5432).unwrap();
        assert_eq!(pg.addresses.len(), 2);
    }
}

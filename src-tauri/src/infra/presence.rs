use super::model::{Phase, Snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallState {
    NotInstalled,
    InstalledStopped,
    Present,
}

pub fn gate(name: &str, probe: &str) -> Option<Snapshot> {
    if probe.trim().is_empty() || always_on(name) {
        return None;
    }
    match classify(name, probe) {
        InstallState::Present => None,
        InstallState::NotInstalled => Some(Snapshot::problem(
            name,
            Phase::NotInstalled,
            &not_installed_message(name),
            "Nothing matching this collector was found. It is not counted as unavailable.",
        )),
        InstallState::InstalledStopped => Some(Snapshot::problem(
            name,
            Phase::InstalledStopped,
            &format!("{} is installed but not running.", title(name)),
            "Start the service on the server if you want these metrics. Tracelyn does not start it.",
        )),
    }
}

fn always_on(name: &str) -> bool {
    matches!(
        name,
        "certificates"
            | "cron"
            | "capacity"
            | "ssh_security"
            | "ports"
            | "updates"
            | "last_logins"
    )
}

fn not_installed_message(name: &str) -> String {
    if name == "nginx_upstreams" {
        "Nginx is not installed on this server.".into()
    } else {
        format!("{} was not detected on this server.", title(name))
    }
}

fn title(name: &str) -> &'static str {
    match name {
        "nginx_upstreams" => "Nginx",
        "docker_infra" => "Docker",
        "haproxy" => "HAProxy",
        "traefik" => "Traefik",
        "caddy" => "Caddy",
        "mysql" => "MySQL",
        "postgres" => "PostgreSQL",
        "mongo" => "MongoDB",
        "rabbitmq" => "RabbitMQ",
        "kafka" => "Kafka",
        "kubernetes" => "Kubernetes",
        "fail2ban" => "fail2ban",
        _ => "This service",
    }
}

pub fn classify(name: &str, probe: &str) -> InstallState {
    let ev = parse(probe);
    let spec = match name {
        "haproxy" => spec(&["haproxy"], &["haproxy"], &[8404], &["haproxy"]),
        "traefik" => spec(&["traefik"], &["traefik"], &[8080], &["traefik"]),
        "caddy" => spec(&["caddy"], &["caddy"], &[2019], &["caddy"]),
        "nginx_upstreams" => spec(&["nginx"], &["nginx"], &[80, 443], &["nginx"]),
        "docker_infra" => spec(&["docker"], &["docker"], &[], &["docker"]),
        "kubernetes" => spec(&["kubectl", "k3s"], &[], &[], &["k3s", "kubernetes"]),
        "mysql" => spec(&["mysql", "mariadbd"], &["mysql", "mariadb"], &[3306], &["mysql", "mariadb"]),
        "postgres" => spec(&["psql", "postgres"], &["postgresql"], &[5432], &["postgres"]),
        "mongo" => spec(&["mongod", "mongosh"], &["mongod"], &[27017], &["mongo"]),
        "rabbitmq" => spec(&["rabbitmqctl"], &["rabbitmq-server"], &[5672], &["rabbitmq"]),
        "kafka" => spec(&["kafka-topics"], &[], &[9092], &["kafka"]),
        "fail2ban" => spec(&["fail2ban-client"], &["fail2ban"], &[], &[]),
        _ => return InstallState::Present,
    };
    let installed = spec.bins.iter().any(|b| ev.bins.iter().any(|have| have == b))
        || spec.units.iter().any(|u| ev.units.iter().any(|(name, _)| name == u))
        || spec.ports.iter().any(|p| listens(&ev.ss, *p))
        || spec.docker.iter().any(|d| ev.docker.contains(d));
    if !installed {
        return InstallState::NotInstalled;
    }
    let running = spec.units.iter().any(|u| {
        ev.units.iter().any(|(name, state)| name == u && state == "active")
    }) || spec.ports.iter().any(|p| listens(&ev.ss, *p))
        || spec.docker.iter().any(|d| ev.docker.contains(d));
    let stopped = spec.units.iter().any(|u| {
        ev.units
            .iter()
            .any(|(name, state)| name == u && matches!(state.as_str(), "inactive" | "failed" | "dead"))
    });
    if running {
        InstallState::Present
    } else if stopped {
        InstallState::InstalledStopped
    } else {
        InstallState::Present
    }
}

struct Spec {
    bins: &'static [&'static str],
    units: &'static [&'static str],
    ports: &'static [u16],
    docker: &'static [&'static str],
}

fn spec(
    bins: &'static [&'static str],
    units: &'static [&'static str],
    ports: &'static [u16],
    docker: &'static [&'static str],
) -> Spec {
    Spec { bins, units, ports, docker }
}

struct Evidence {
    bins: Vec<String>,
    units: Vec<(String, String)>,
    ss: String,
    docker: String,
}

fn parse(text: &str) -> Evidence {
    let mut section = "";
    let mut ev = Evidence {
        bins: Vec::new(),
        units: Vec::new(),
        ss: String::new(),
        docker: String::new(),
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("---") {
            section = line;
            continue;
        }
        if line.is_empty() {
            continue;
        }
        match section {
            "---bins---" => ev.bins.push(line.to_string()),
            "---units---" => {
                let mut parts = line.split_whitespace();
                if let Some(name) = parts.next() {
                    let state = parts.next().unwrap_or("unknown").to_string();
                    ev.units.push((name.to_string(), state));
                }
            }
            "---ss---" => {
                ev.ss.push_str(line);
                ev.ss.push('\n');
            }
            "---docker---" => {
                ev.docker.push_str(&line.to_ascii_lowercase());
                ev.docker.push('\n');
            }
            _ => {}
        }
    }
    ev
}

fn listens(ss: &str, port: u16) -> bool {
    ss.split_whitespace().any(|tok| {
        let parsed = if let Some(rest) = tok.strip_prefix('[') {
            rest.split_once("]:").map(|(_, p)| p)
        } else {
            tok.rsplit_once(':').map(|(_, p)| p)
        };
        parsed.and_then(|p| p.parse::<u16>().ok()) == Some(port)
    })
}

pub fn nginx_needs_root(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("permission denied")
        || lower.contains("must be root")
        || lower.contains("need root")
        || lower.contains("open() ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_balancer_is_not_installed() {
        let probe = "---bins---\nnginx\n---units---\nnginx active\n---ss---\n0.0.0.0:80\n---docker---\n";
        assert_eq!(classify("haproxy", probe), InstallState::NotInstalled);
        assert_eq!(classify("traefik", probe), InstallState::NotInstalled);
        assert_eq!(classify("caddy", probe), InstallState::NotInstalled);
        let snap = gate("haproxy", probe).unwrap();
        assert_eq!(snap.phase, Phase::NotInstalled);
    }

    #[test]
    fn nginx_stopped_and_running() {
        let stopped = "---bins---\nnginx\n---units---\nnginx inactive\n---ss---\n---docker---\n";
        assert_eq!(classify("nginx_upstreams", stopped), InstallState::InstalledStopped);
        let running = "---bins---\nnginx\n---units---\nnginx active\n---ss---\n0.0.0.0:80\n---docker---\n";
        assert_eq!(classify("nginx_upstreams", running), InstallState::Present);
        assert!(gate("nginx_upstreams", running).is_none());
        let absent = "---bins---\n---units---\n---ss---\n---docker---\n";
        assert_eq!(classify("nginx_upstreams", absent), InstallState::NotInstalled);
        assert_eq!(
            gate("nginx_upstreams", absent).unwrap().message,
            "Nginx is not installed on this server."
        );
    }

    #[test]
    fn docker_image_counts_as_present() {
        let probe = "---bins---\n---units---\n---ss---\n---docker---\ntraefik:v3 edge\n";
        assert_eq!(classify("traefik", probe), InstallState::Present);
    }
}

use crate::apps::types::TracelynApp;
use crate::events::SourceStatus;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEntry {
    pub collector: String,
    pub status: DiagnosticStatus,
    pub message: String,
    pub fix_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticStatus {
    Ok,
    Warning,
    Error,
}

pub fn build_diagnostics(
    apps: &[TracelynApp],
    web_detected: bool,
    web_logs_ok: bool,
    unreadable_logs: &[String],
    redis_phase: &str,
    source_status: &std::collections::HashMap<String, SourceStatus>,
) -> Vec<DiagnosticEntry> {
    let pm2 = apps.iter().filter(|a| a.kind == crate::apps::types::AppKind::Pm2).count();
    let systemd = apps
        .iter()
        .filter(|a| a.kind == crate::apps::types::AppKind::Systemd)
        .count();
    let docker = apps
        .iter()
        .filter(|a| a.kind == crate::apps::types::AppKind::Docker)
        .count();
    let nginx = apps
        .iter()
        .any(|a| a.kind == crate::apps::types::AppKind::Nginx)
        || web_detected;
    let apache = apps
        .iter()
        .any(|a| a.kind == crate::apps::types::AppKind::Apache);

    let mut entries = vec![
        DiagnosticEntry {
            collector: "system".into(),
            status: DiagnosticStatus::Ok,
            message: "System metrics collector is running".into(),
            fix_hint: None,
        },
        DiagnosticEntry {
            collector: "apps".into(),
            status: if apps.is_empty() {
                DiagnosticStatus::Warning
            } else {
                DiagnosticStatus::Ok
            },
            message: format!("{} application(s) in the registry", apps.len()),
            fix_hint: if apps.is_empty() {
                Some("Connect the server, or add an application manually.".into())
            } else {
                None
            },
        },
        count_entry("pm2", pm2, "PM2 processes", "Run `pm2 jlist` as the deploy user."),
        count_entry(
            "systemd",
            systemd,
            "custom systemd units",
            "Put the unit file in /etc/systemd/system and start it.",
        ),
        count_entry(
            "docker",
            docker,
            "containers",
            "Start the container or grant the SSH user access to the docker socket.",
        ),
        DiagnosticEntry {
            collector: "nginx".into(),
            status: if nginx {
                DiagnosticStatus::Ok
            } else {
                DiagnosticStatus::Warning
            },
            message: if nginx {
                "Nginx config or vhost detected".into()
            } else {
                "Nginx not detected".into()
            },
            fix_hint: None,
        },
        DiagnosticEntry {
            collector: "apache".into(),
            status: if apache {
                DiagnosticStatus::Ok
            } else {
                DiagnosticStatus::Warning
            },
            message: if apache {
                "Apache vhost detected".into()
            } else {
                "Apache not detected".into()
            },
            fix_hint: None,
        },
        DiagnosticEntry {
            collector: "logs".into(),
            status: if web_logs_ok {
                DiagnosticStatus::Ok
            } else if web_detected {
                DiagnosticStatus::Error
            } else {
                DiagnosticStatus::Warning
            },
            message: if web_logs_ok {
                "Access log tail is active".into()
            } else if unreadable_logs.is_empty() {
                "No access log is being tailed".into()
            } else {
                format!("Cannot read {}", unreadable_logs.join(", "))
            },
            fix_hint: Some(
                "Add the SSH user to the adm group, then reconnect.".into(),
            ),
        },
        DiagnosticEntry {
            collector: "redis".into(),
            status: match redis_phase {
                "live" => DiagnosticStatus::Ok,
                "loading" => DiagnosticStatus::Warning,
                "not_installed" => DiagnosticStatus::Warning,
                "auth_required" => DiagnosticStatus::Warning,
                _ => DiagnosticStatus::Error,
            },
            message: format!("Redis collector: {redis_phase}"),
            fix_hint: match redis_phase {
                "auth_required" => {
                    Some("Set the Redis password (and ACL user, if any) on the Redis page.".into())
                }
                "connection_refused" => Some(
                    "Check the port, unix socket, or Docker container name on the Redis page."
                        .into(),
                ),
                "timed_out" => Some("Redis did not answer within 5s. Confirm it is running.".into()),
                "not_installed" => {
                    Some("Install Redis, or point Tracelyn at a Docker container running it.".into())
                }
                _ => None,
            },
        },
    ];

    for (id, status) in source_status {
        entries.push(DiagnosticEntry {
            collector: format!("logs:{id}"),
            status: if status.ok {
                DiagnosticStatus::Ok
            } else {
                DiagnosticStatus::Error
            },
            message: status
                .reason
                .clone()
                .unwrap_or_else(|| format!("{id} is streaming")),
            fix_hint: status.fix_hint.clone(),
        });
    }
    entries
}

fn count_entry(name: &str, count: usize, noun: &str, hint: &str) -> DiagnosticEntry {
    DiagnosticEntry {
        collector: name.into(),
        status: if count > 0 {
            DiagnosticStatus::Ok
        } else {
            DiagnosticStatus::Warning
        },
        message: if count > 0 {
            format!("{count} {noun}")
        } else {
            format!("No {noun} detected")
        },
        fix_hint: if count > 0 { None } else { Some(hint.into()) },
    }
}

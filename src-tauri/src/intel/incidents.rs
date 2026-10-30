//! Durable incident identity. Recovery is handled per issue, never per server.
use super::{cycle::Notice, store, types::IntelError};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;

pub fn migrate(conn: &Connection) -> Result<(), IntelError> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS incident_identity (
        server_id TEXT NOT NULL, issue_key TEXT NOT NULL, incident_id TEXT NOT NULL,
        PRIMARY KEY(server_id, issue_key));
        CREATE TABLE IF NOT EXISTS incident_sources (server_id TEXT NOT NULL, issue_key TEXT NOT NULL,
            source_id TEXT NOT NULL, active INTEGER NOT NULL, PRIMARY KEY(server_id,issue_key,source_id));
        CREATE TABLE IF NOT EXISTS incident_last_metrics (server_id TEXT PRIMARY KEY, ts_ms INTEGER NOT NULL, metrics TEXT NOT NULL);")?;
    Ok(())
}

pub fn guidance(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "server_unreachable" => (
            "The connection to this server was lost. The server or its network may be unavailable.",
            "Check your internet connection, then check the server status in your hosting account.",
        ),
        "app_down" | "app_crash" | "service_stopped" => (
            "An application or service has stopped.",
            "Open Apps and review the stopped service and its recent logs before restarting it.",
        ),
        "restart_loop" => (
            "The application keeps stopping and restarting.",
            "Open its logs and review the first error after each restart.",
        ),
        "ssl_expiry" => (
            "The website certificate needs renewal.",
            "Renew the certificate and check that the website opens securely.",
        ),
        "disk_forecast" | "disk_high" => (
            "Files are using most of the available storage.",
            "Review storage use and remove unneeded files or increase disk space.",
        ),
        "cpu_high" => (
            "The server is doing more work than usual.",
            "Open Processes to find the busiest application.",
        ),
        "memory_high" | "oom" => (
            "Applications may need more memory than the server has available.",
            "Review application memory use and recent logs, then reduce load or increase memory.",
        ),
        "uptime" => (
            "The website did not pass its availability check.",
            "Check the website, its application, and the web server.",
        ),
        _ => (
            "A monitoring rule detected an unusual value. The cause is not confirmed.",
            "Review the incident timeline and the related logs before making changes.",
        ),
    }
}

pub fn is_oom_kill(line: &str) -> bool {
    let text = line.to_ascii_lowercase();
    text.contains("oom-kill:")
        || text.contains("out of memory: killed process")
        || text.contains("memory cgroup out of memory: killed process")
}

pub fn issue_key(kind: &str, target: &str) -> String {
    let kind = match (kind, target) {
        ("app_crash", _) => "app_down",
        ("metric", "cpu.percent") => "cpu_high",
        ("metric", "memory.percent") => "memory_high",
        ("metric", "disk.percent") => "disk_high",
        _ => kind,
    };
    let target = if kind == "server_unreachable" {
        "server"
    } else {
        target
    };
    serde_json::json!([kind, target]).to_string()
}

pub fn notification_text(private: bool, title: &str, body: &str) -> (String, String) {
    if private {
        (
            if title.starts_with("Resolved:") {
                "Resolved incident"
            } else {
                "Monitoring incident"
            }
            .into(),
            "Open the incident in Tracelyn for details".into(),
        )
    } else {
        (title.replace(';', ","), body.replace(';', ","))
    }
}

pub fn open(
    conn: &Connection,
    server: &str,
    name: &str,
    key: &str,
    source: &str,
    kind: &str,
    title: &str,
    message: &str,
    severity: &str,
    ts: i64,
    metrics: &HashMap<String, f64>,
) -> Result<Option<Notice>, IntelError> {
    migrate(conn)?;
    let name = name.replace(';', ",");
    conn.execute("INSERT INTO incident_sources VALUES (?1,?2,?3,1) ON CONFLICT(server_id,issue_key,source_id) DO UPDATE SET active=1", params![server,key,source])?;
    let existing: Option<String> = conn
        .query_row(
            "SELECT i.id FROM incident_identity k JOIN intel_incidents i ON i.id=k.incident_id
         WHERE k.server_id=?1 AND k.issue_key=?2 AND i.status!='resolved'",
            params![server, key],
            |r| r.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Ok(None);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let title = if kind == "server_unreachable" {
        format!("Server down: {name}")
    } else {
        format!("{title}: {name}")
    }
    .replace(';', ",");
    conn.execute("INSERT INTO intel_incidents (id,server_id,title,severity,status,opened_at_ms,updated_at_ms,kind)
        VALUES (?1,?2,?3,?4,'open',?5,?5,?6)", params![id,server,title,severity,ts,kind])?;
    conn.execute("INSERT INTO incident_identity VALUES (?1,?2,?3) ON CONFLICT(server_id,issue_key) DO UPDATE SET incident_id=excluded.incident_id", params![server,key,id])?;
    let (cause, next) = guidance(kind);
    store::add_timeline(conn, &id, ts, "alert", &message.replace(';', ","))?;
    store::add_timeline(conn, &id, ts, "cause", cause)?;
    store::add_timeline(conn, &id, ts, "next_step", next)?;
    if kind == "server_unreachable" {
        if let Some(up) = metrics.get("server.ping_up") {
            store::add_timeline(
                conn,
                &id,
                ts,
                "check",
                if *up >= 1.0 {
                    "The server replied to ping. Its secure connection is unavailable"
                } else {
                    "Ping did not reply. Some servers block ping, so this does not confirm that the server is powered off"
                },
            )?;
        }
    }
    let mut values: Vec<_> = metrics.iter().filter(|(_, v)| v.is_finite()).collect();
    values.sort_by_key(|(key, _)| *key);
    let current = values
        .iter()
        .map(|(k, v)| format!("{k}: {v:.2}"))
        .collect::<Vec<_>>()
        .join(", ");
    let (measured_at, snapshot): (i64, String) = conn
        .query_row(
            "SELECT ts_ms,metrics FROM incident_last_metrics WHERE server_id=?1",
            [server],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .unwrap_or((ts, current));
    store::add_timeline(
        conn,
        &id,
        measured_at,
        "metrics",
        if snapshot.is_empty() {
            "No recent measurements were available before this failure"
        } else {
            &snapshot
        },
    )?;
    let since = chrono::DateTime::from_timestamp_millis(ts)
        .map(|v| v.to_rfc3339())
        .unwrap_or_else(|| ts.to_string());
    let message = message.replace(';', ",");
    let mut body = if message.starts_with(&format!("{name}:")) {
        message
    } else {
        format!("{name}: {message}")
    };
    if !body.contains(". Since ") {
        body.push_str(&format!(". Since {since}"));
    }
    Ok(Some(Notice {
        title,
        body,
        path: format!("/intel/incidents/{id}"),
    }))
}

pub fn resolve(
    conn: &Connection,
    server: &str,
    name: &str,
    key: &str,
    source: &str,
    ts: i64,
) -> Result<Option<Notice>, IntelError> {
    migrate(conn)?;
    conn.execute(
        "UPDATE incident_sources SET active=0 WHERE server_id=?1 AND issue_key=?2 AND source_id=?3",
        params![server, key, source],
    )?;
    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM incident_sources WHERE server_id=?1 AND issue_key=?2 AND active=1",
        params![server, key],
        |r| r.get(0),
    )?;
    if active > 0 {
        return Ok(None);
    }
    let row: Option<(String,String,i64)> = conn.query_row("SELECT i.id,i.title,i.opened_at_ms FROM incident_identity k
        JOIN intel_incidents i ON i.id=k.incident_id WHERE k.server_id=?1 AND k.issue_key=?2 AND i.status!='resolved'",
        params![server,key], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((id, title, start)) = row else {
        return Ok(None);
    };
    store::set_incident_status(conn, &id, "resolved", ts)?;
    let body = format!(
        "{name} recovered. The issue lasted {} seconds",
        (ts - start).max(0) / 1000
    )
    .replace(';', ",");
    store::add_timeline(conn, &id, ts, "resolved", &body)?;
    Ok(Some(Notice {
        title: format!("Resolved: {title}"),
        body,
        path: format!("/intel/incidents/{id}"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn issue_keys_match_across_engines_and_keep_targets_separate() {
        assert_eq!(
            issue_key("metric", "cpu.percent"),
            issue_key("cpu_high", "cpu.percent")
        );
        assert_eq!(issue_key("app_crash", "api"), issue_key("app_down", "api"));
        assert_ne!(issue_key("app_down", "api"), issue_key("app_down", "web"));
        assert_ne!(issue_key("uptime", "a:443"), issue_key("uptime", "b:443"));
    }
    #[test]
    fn private_notifications_hide_every_detail() {
        let (title, body) = notification_text(
            true,
            "Server down: secret-host",
            "root at 10.0.0.1 with /keys/private",
        );
        assert_eq!(title, "Monitoring incident");
        assert!(!body.contains("root") && !body.contains("10.0.0.1") && !body.contains("/keys"));
        let (title, body) = notification_text(
            false,
            "Server down: Production",
            "Connection lost, check hosting",
        );
        assert!(title.contains("Production"));
        assert!(!body.contains(';'));
    }
    #[test]
    fn oom_parser_requires_a_kernel_kill_record() {
        assert!(is_oom_kill(
            "kernel: Out of memory: Killed process 42 (node)"
        ));
        assert!(is_oom_kill("kernel: oom-kill: constraint=CONSTRAINT_MEMCG"));
        assert!(!is_oom_kill(
            "Application warns that it might run out of memory"
        ));
        assert!(!is_oom_kill("No OOM kills detected"));
    }
    #[test]
    fn identity_dedupes_and_resolves_only_one_target() {
        let c = Connection::open_in_memory().unwrap();
        store::migrate(&c).unwrap();
        let m = HashMap::new();
        let fire = |key| {
            open(
                &c,
                "s",
                "Production",
                key,
                key,
                "server_unreachable",
                "Down",
                "Connection lost",
                "critical",
                1000,
                &m,
            )
            .unwrap()
        };
        let n = fire("ssh").unwrap();
        assert!(n.title.contains("Production"));
        assert!(!n.body.contains(';'));
        assert!(fire("ssh").is_none());
        assert!(fire("http").is_some());
        assert!(resolve(&c, "s", "Production", "ssh", "ssh", 32000)
            .unwrap()
            .unwrap()
            .body
            .contains("31 seconds"));
        assert!(resolve(&c, "s", "Production", "ssh", "ssh", 33000)
            .unwrap()
            .is_none());
        assert_eq!(
            store::list_incidents(&c, None)
                .unwrap()
                .iter()
                .filter(|i| i.status != "resolved")
                .count(),
            1
        );
    }
    #[test]
    fn shared_issue_waits_for_every_detector_to_recover() {
        let c = Connection::open_in_memory().unwrap();
        store::migrate(&c).unwrap();
        let m = HashMap::new();
        assert!(open(
            &c,
            "s",
            "Production",
            "cpu",
            "rule-one",
            "cpu_high",
            "High CPU",
            "Busy",
            "critical",
            1000,
            &m
        )
        .unwrap()
        .is_some());
        assert!(open(
            &c,
            "s",
            "Production",
            "cpu",
            "rule-two",
            "cpu_high",
            "High CPU",
            "Busy",
            "critical",
            2000,
            &m
        )
        .unwrap()
        .is_none());
        assert!(resolve(&c, "s", "Production", "cpu", "rule-one", 3000)
            .unwrap()
            .is_none());
        assert!(resolve(&c, "s", "Production", "cpu", "rule-two", 4000)
            .unwrap()
            .is_some());
    }
    #[test]
    fn causes_and_actions_are_plain_text() {
        for kind in [
            "server_unreachable",
            "app_down",
            "restart_loop",
            "ssl_expiry",
            "disk_high",
            "cpu_high",
            "memory_high",
            "uptime",
            "unknown",
        ] {
            let (cause, action) = guidance(kind);
            assert!(!cause.is_empty() && !action.is_empty());
            assert!(!format!("{cause}{action}").contains(';'));
        }
    }
    #[test]
    fn migrations_are_repeatable_and_keep_incident_identity() {
        let c = Connection::open_in_memory().unwrap();
        store::migrate(&c).unwrap();
        open(
            &c,
            "s",
            "Production",
            "cpu",
            "rule-cpu",
            "cpu_high",
            "High CPU",
            "Busy",
            "warning",
            1000,
            &HashMap::new(),
        )
        .unwrap();
        migrate(&c).unwrap();
        store::migrate(&c).unwrap();
        assert!(open(
            &c,
            "s",
            "Production",
            "cpu",
            "rule-cpu",
            "cpu_high",
            "High CPU",
            "Busy",
            "warning",
            2000,
            &HashMap::new()
        )
        .unwrap()
        .is_none());
        assert_eq!(store::list_incidents(&c, None).unwrap()[0].kind, "cpu_high");
    }
}

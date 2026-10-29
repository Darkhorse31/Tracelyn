//! Poll only connected servers, import server events idempotently, and merge history.
use crate::state::AppState;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
fn leases() -> &'static Mutex<HashMap<String, i64>> {
    static LEASES: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    LEASES.get_or_init(Default::default)
}
pub fn forget(server: &str) {
    leases().lock().unwrap().remove(server);
}
pub fn owns_basic(server: &str, rule: &crate::alerts::AlertRule) -> bool {
    leases().lock().unwrap().get(server).copied().unwrap_or(0) > crate::intel::now_ms()
        && matches!(rule.kind.as_str(), "cpu_high" | "memory_high" | "disk_high")
        && rule
            .params
            .metric_key
            .as_deref()
            .map(|k| {
                matches!(
                    k,
                    "cpu.percent" | "memory.percent" | "disk.percent" | "load.1"
                )
            })
            .unwrap_or(true)
}
pub fn owns_rule(server: &str, rule: &tracelyn_core::types::IntelRule) -> bool {
    let live = leases().lock().unwrap().get(server).copied().unwrap_or(0) > crate::intel::now_ms();
    live && match rule.kind.as_str() {
        "metric" => matches!(
            rule.params.metric_key.as_str(),
            "" | "cpu.percent"
                | "memory.percent"
                | "disk.percent"
                | "load.1"
                | "load.5"
                | "load.15"
                | "net.rx_bps"
                | "net.tx_bps"
                | "disk.read_bps"
                | "disk.write_bps"
        ),
        "uptime" | "ssl_expiry" | "log_pattern" => true,
        _ => false,
    }
}
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            let ids = state
                .config
                .lock()
                .list_servers()
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>();
            for id in ids {
                if !matches!(
                    state.runtime(&id).status,
                    crate::state::ConnectionStatus::Connected
                        | crate::state::ConnectionStatus::Degraded
                ) {
                    continue;
                }
                let _ = poll(&app, &state, &id).await;
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}
async fn poll(app: &AppHandle, state: &AppState, id: &str) -> Result<(), String> {
    let status=crate::commands::companion::remote(state,id,"test -x /usr/local/bin/tracelyn-companion && /usr/local/bin/tracelyn-companion status --json").await?;
    let status: Value = serde_json::from_str(&status).map_err(|e| e.to_string())?;
    let now = crate::intel::now_ms();
    let matches = crate::commands::companion::rules_hash(state, id)
        .ok()
        .as_deref()
        == status["rulesHash"].as_str();
    if !matches {
        let _ = crate::commands::companion::sync_one(app, state, id).await;
        leases().lock().unwrap().remove(id);
    }
    if matches
        && status["heartbeatAgeMs"]
            .as_i64()
            .map(|age| (0..20_000).contains(&age))
            .unwrap_or(false)
    {
        leases().lock().unwrap().insert(id.into(), now + 60_000);
    }
    let since = {
        let db = state.metrics.lock();
        crate::intel::migrate(db.connection()).map_err(|e| e.to_string())?;
        db.connection()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS companion_cursor(server TEXT PRIMARY KEY,ts INTEGER)",
            )
            .map_err(|e| e.to_string())?;
        db.connection()
            .query_row(
                "SELECT ts FROM companion_cursor WHERE server=?1",
                [id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(now - 30 * 86_400_000)
    };
    let events = crate::commands::companion::remote(
        state,
        id,
        &format!("/usr/local/bin/tracelyn-companion events --since {since} --json"),
    )
    .await?;
    let events: Vec<Value> = serde_json::from_str(&events).map_err(|e| e.to_string())?;
    {
        let db = state.metrics.lock();
        import_events(db.connection(), id, &events)?;
    }
    if events
        .iter()
        .any(|e| e["tsMs"].as_i64().unwrap_or(0) > since)
    {
        let _ = app.emit("companion-history-updated", id);
    }
    let cursor = events
        .last()
        .and_then(|e| e["tsMs"].as_i64())
        .unwrap_or(since);
    state.metrics.lock().connection().execute("INSERT INTO companion_cursor VALUES (?1,?2) ON CONFLICT(server) DO UPDATE SET ts=excluded.ts",rusqlite::params![id,cursor]).map_err(|e|e.to_string())?;
    Ok(())
}
fn import_events(db: &rusqlite::Connection, server: &str, events: &[Value]) -> Result<(), String> {
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    for e in events {
        let ts = e["tsMs"].as_i64().ok_or("Invalid event timestamp")?;
        let remote_id = e["id"].as_str().ok_or("Invalid event ID")?;
        let id = format!("companion:{server}:{remote_id}");
        let title = e["title"].as_str().unwrap_or("Server alert");
        let severity = e["severity"].as_str().unwrap_or("warning");
        let sent = e["sentChannels"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        let message = format!(
            "{} · {}",
            title,
            if sent {
                "Sent by server"
            } else {
                "Server delivery pending or failed"
            }
        );
        let firing = e["firing"].as_bool().unwrap_or(true);
        let run = || -> rusqlite::Result<()> {
            let n = tx.execute(
                "INSERT OR IGNORE INTO intel_feed VALUES (?1,?2,?3,'companion',?4)",
                rusqlite::params![id, server, ts, message],
            )?;
            if n == 0 {
                return Ok(());
            }
            let rule = e["ruleId"].as_str().unwrap_or(remote_id);
            tx.execute("INSERT INTO intel_rule_state(rule_id,server_id,firing,last_value,checked_at_ms,last_notify_ms) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(rule_id,server_id) DO UPDATE SET firing=excluded.firing,last_value=excluded.last_value,checked_at_ms=excluded.checked_at_ms,last_notify_ms=COALESCE(excluded.last_notify_ms,intel_rule_state.last_notify_ms) WHERE intel_rule_state.checked_at_ms<=excluded.checked_at_ms",rusqlite::params![rule,server,firing,e["value"].as_f64(),ts,if sent{Some(ts)}else{None}])?;
            if firing {
                let open:Option<String>=tx.query_row("SELECT id FROM alert_events WHERE rule_id=?1 AND server_id=?2 AND resolved_at_ms IS NULL AND id LIKE 'companion:%' LIMIT 1",rusqlite::params![rule,server],|r|r.get(0)).ok();
                if let Some(open) = open {
                    tx.execute(
                        "UPDATE alert_events SET message=?1 WHERE id=?2",
                        rusqlite::params![message, open],
                    )?;
                } else {
                    tx.execute("INSERT OR IGNORE INTO alert_events(id,rule_id,rule_name,server_id,severity,message,value,threshold,fired_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",rusqlite::params![id,rule,title,server,severity,message,e["value"].as_f64().unwrap_or(0.),e["threshold"].as_f64().unwrap_or(0.),ts])?;
                    tx.execute(
                        "INSERT OR IGNORE INTO intel_seen_alert VALUES (?1,?2)",
                        rusqlite::params![id, ts],
                    )?;
                }
            } else {
                tx.execute("UPDATE alert_events SET resolved_at_ms=?1 WHERE rule_id=?2 AND server_id=?3 AND resolved_at_ms IS NULL AND id LIKE 'companion:%'",rusqlite::params![ts,rule,server])?;
            }
            // One incident per server and rule, reopening creates a new incident.
            let prefix = format!("companion:{server}:{rule}:%");
            if firing {
                let open:Option<String>=tx.query_row("SELECT id FROM intel_incidents WHERE id LIKE ?1 AND status!='resolved' ORDER BY opened_at_ms DESC LIMIT 1",[&prefix],|r|r.get(0)).ok();
                let incident = open.unwrap_or_else(|| id.clone());
                tx.execute("INSERT OR IGNORE INTO intel_incidents(id,server_id,title,severity,status,opened_at_ms,updated_at_ms,unread) VALUES (?1,?2,?3,?4,'open',?5,?5,1)",rusqlite::params![incident,server,title,severity,ts])?;
                tx.execute(
                    "INSERT OR IGNORE INTO intel_timeline VALUES (?1,?2,?3,'companion',?4)",
                    rusqlite::params![id, incident, ts, message],
                )?;
            } else {
                let incident:Option<String>=tx.query_row("SELECT id FROM intel_incidents WHERE id LIKE ?1 AND status!='resolved' ORDER BY opened_at_ms DESC LIMIT 1",[prefix],|r|r.get(0)).ok();
                if let Some(incident) = incident {
                    tx.execute("UPDATE intel_incidents SET status='resolved',resolved_at_ms=?1,updated_at_ms=?1 WHERE id=?2",rusqlite::params![ts,incident])?;
                    tx.execute(
                        "INSERT OR IGNORE INTO intel_timeline VALUES (?1,?2,?3,'companion',?4)",
                        rusqlite::params![id, incident, ts, message],
                    )?;
                }
            }
            Ok(())
        };
        run().map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_server_events_once_and_resolves() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        crate::intel::migrate(&db).unwrap();
        crate::alerts::AlertHistoryStore::migrate(&db).unwrap();
        let fired = serde_json::json!({"id":"cpu:1000","ruleId":"cpu","title":"CPU high","severity":"critical","tsMs":1000,"value":95.,"threshold":90.,"firing":true,"sentChannels":["slack"]});
        import_events(&db, "s", &[fired.clone()]).unwrap();
        import_events(&db, "s", &[fired]).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM intel_incidents", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM intel_feed", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(db
            .query_row("SELECT message FROM alert_events", [], |r| r
                .get::<_, String>(0))
            .unwrap()
            .contains("Sent by server"));
        let clear = serde_json::json!({"id":"cpu:50000","ruleId":"cpu","title":"CPU cleared","severity":"critical","tsMs":50000,"value":20.,"firing":false,"sentChannels":[]});
        import_events(&db, "s", &[clear]).unwrap();
        assert_eq!(
            db.query_row("SELECT status FROM intel_incidents", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "resolved"
        );
        assert_eq!(
            db.query_row("SELECT resolved_at_ms FROM alert_events", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            50000
        );
    }
    #[test]
    fn rule_evaluation_matches_desktop_for_threshold_duration_and_clear() {
        use tracelyn_core::{
            logic::Gate,
            types::{IntelRule, RuleParams},
        };
        for kind in ["metric", "ssl_expiry", "uptime"] {
            let rule = IntelRule {
                id: "r".into(),
                server_id: None,
                kind: kind.into(),
                name: "Test".into(),
                enabled: true,
                severity: "warning".into(),
                params: RuleParams {
                    threshold: 10.,
                    duration_secs: 10,
                    clear_duration_secs: 30,
                    ..Default::default()
                },
                created_at_ms: 0,
                updated_at_ms: 0,
                firing: false,
            };
            let db = rusqlite::Connection::open_in_memory().unwrap();
            crate::intel::migrate(&db).unwrap();
            crate::intel::store::upsert_rule(&db, &rule).unwrap();
            let mut server = Gate::default();
            for (ts, value) in [
                (0, 20.),
                (5000, 5.),
                (15000, 5.),
                (20000, 20.),
                (30000, 20.),
                (60000, 20.),
                (70000, 5.),
                (110000, 5.),
            ] {
                let metrics = HashMap::from([("cpu.percent".into(), value)]);
                let probes = HashMap::from([("r".into(), Ok(value))]);
                let sample = crate::intel::cycle::Sample {
                    server_id: "parity",
                    server_name: "Test",
                    ts_ms: ts,
                    metrics: &metrics,
                    logs: &[],
                    restarts: &[],
                    apps: &[],
                    services: &[],
                    fail_count: None,
                    login_lines: None,
                    utc_offset_secs: 0,
                    probes: &probes,
                    eval_metrics: true,
                    eval_logs: true,
                    eval_anomaly: false,
                };
                crate::intel::cycle::run(&db, &sample).unwrap();
                let state = crate::intel::store::load_state(&db, "r", "parity").unwrap();
                let desktop = Gate {
                    firing: state.firing,
                    condition_since_ms: state.condition_since_ms,
                    clear_since_ms: state.clear_since_ms,
                };
                server = tracelyn_companion::evaluate(&rule, server, value, ts);
                assert_eq!(desktop, server, "{kind} at {ts}");
            }
        }
    }
}
pub async fn merge(
    state: &AppState,
    mut local: crate::storage::QueryResult,
    start: i64,
    end: i64,
) -> crate::storage::QueryResult {
    if !matches!(
        state.runtime(&local.server_id).status,
        crate::state::ConnectionStatus::Connected | crate::state::ConnectionStatus::Degraded
    ) {
        return local;
    }
    let metric = &local.metric_key;
    if metric.len() > 100
        || metric
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || "._-".contains(c)))
    {
        return local;
    }
    let cmd=format!("/usr/local/bin/tracelyn-companion query --since {start} --until {end} --metric '{metric}' --json");
    if let Ok(data) = crate::commands::companion::remote(state, &local.server_id, &cmd).await {
        if let Ok(points) = serde_json::from_str::<Vec<tracelyn_companion::storage::Point>>(&data) {
            let mut merged = BTreeMap::new();
            for p in points {
                merged.insert(
                    p.ts_ms,
                    crate::storage::MetricPoint {
                        ts_ms: p.ts_ms,
                        value: p.value,
                        min: Some(p.min),
                        max: Some(p.max),
                    },
                );
            }
            for p in local.points {
                merged.insert(p.ts_ms, p);
            }
            local.points = merged.into_values().collect();
        }
    }
    local
}

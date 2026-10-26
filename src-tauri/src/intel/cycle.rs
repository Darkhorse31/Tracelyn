//! One evaluation pass: rules, seasonal anomalies, restarts, and alert grouping.

use super::logic::{
    advance_gate, clear_level, clip_chars, hot_cool, hour_of_week, log_rate_per_min, restart_increase,
    step_model, Gate,
};
use super::store;
use super::types::{
    default_metric_key, is_low_kind, AnomalyEvent, IntelError, IntelRule, RuleRuntime, WATCH_METRICS,
};
use regex::Regex;
use rusqlite::Connection;
use std::collections::HashMap;
use uuid::Uuid;

pub struct LogObs {
    pub id: String,
    pub ts_ms: i64,
    pub text: String,
}

pub struct RestartObs {
    pub id: String,
    pub name: String,
    pub restarts: u64,
}

pub struct Sample<'a> {
    pub server_id: &'a str,
    pub server_name: &'a str,
    pub ts_ms: i64,
    pub metrics: &'a HashMap<String, f64>,
    pub logs: &'a [LogObs],
    pub restarts: &'a [RestartObs],
    /// Fresh probe results for ssl/uptime rules. Missing keys reuse the last value.
    pub probes: &'a HashMap<String, Result<f64, String>>,
    pub eval_metrics: bool,
    pub eval_logs: bool,
    pub eval_anomaly: bool,
}

#[derive(Debug, Clone)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Default)]
pub struct CycleOut {
    pub fired: u32,
    pub cleared: u32,
    pub notices: Vec<Notice>,
    pub errors: Vec<String>,
}

pub fn run(conn: &Connection, sample: &Sample<'_>) -> Result<CycleOut, IntelError> {
    let tx = conn.unchecked_transaction()?;
    let mut out = CycleOut::default();
    let mut metrics = sample.metrics.clone();
    if sample.eval_metrics {
        for (key, value) in store::latest_metrics(&tx, sample.server_id, sample.ts_ms - 120_000)? {
            metrics.entry(key).or_insert(value);
        }
    }
    let rules = store::list_rules(&tx)?;
    for rule in rules.iter().filter(|rule| rule.enabled && rule_applies(rule, sample.server_id)) {
        if let Err(err) = eval_rule(&tx, rule, sample, &metrics, &mut out) {
            out.errors.push(format!("{}: {err}", rule.name));
        }
    }
    if sample.eval_anomaly {
        for key in WATCH_METRICS {
            if let Some(value) = metrics.get(*key).copied() {
                if let Err(err) = observe_metric(&tx, sample, key, value) {
                    out.errors.push(format!("{key}: {err}"));
                }
            }
        }
    }
    observe_restarts(&tx, sample)?;
    out.notices.extend(fold_alerts(&tx, sample)?);
    tx.commit()?;
    if out.notices.len() > 10 {
        out.notices.truncate(10);
    }
    Ok(out)
}

fn rule_applies(rule: &IntelRule, server_id: &str) -> bool {
    rule.server_id.as_ref().map(|id| id == server_id).unwrap_or(true)
}

fn eval_rule(
    conn: &Connection,
    rule: &IntelRule,
    sample: &Sample<'_>,
    metrics: &HashMap<String, f64>,
    out: &mut CycleOut,
) -> Result<(), IntelError> {
    let mut state = store::load_state(conn, &rule.id, sample.server_id)?;
    let value = match resolve_value(conn, rule, sample, metrics, &mut state)? {
        Some(value) => value,
        None => {
            store::save_state(conn, &rule.id, sample.server_id, &state)?;
            return Ok(());
        }
    };
    let clear = clear_level(&rule.kind, rule.params.threshold, rule.params.clear_threshold);
    let (hot, cooling) = hot_cool(&rule.kind, value, rule.params.threshold, clear);
    let was = state.firing;
    let gate = advance_gate(
        Gate {
            firing: state.firing,
            condition_since_ms: state.condition_since_ms,
            clear_since_ms: state.clear_since_ms,
        },
        hot,
        cooling,
        sample.ts_ms,
        rule.params.duration_secs as i64 * 1000,
        rule.params.clear_duration_secs as i64 * 1000,
    );
    state.firing = gate.firing;
    state.condition_since_ms = gate.condition_since_ms;
    state.clear_since_ms = gate.clear_since_ms;
    state.last_value = Some(value);
    state.last_error = None;
    store::save_state(conn, &rule.id, sample.server_id, &state)?;
    if state.firing && !was {
        let message = alert_message(rule, sample.server_name, value);
        attach_alert(conn, sample, &rule.severity, &rule.name, &message)?;
        out.fired += 1;
        out.notices.push(Notice {
            title: format!("Tracelyn: {}", rule.name),
            body: message,
        });
    } else if !state.firing && was {
        let message = format!("{} cleared ({:.2})", rule.name, value);
        attach_alert(conn, sample, "info", &rule.name, &message)?;
        out.cleared += 1;
    }
    Ok(())
}

fn resolve_value(
    conn: &Connection,
    rule: &IntelRule,
    sample: &Sample<'_>,
    metrics: &HashMap<String, f64>,
    state: &mut RuleRuntime,
) -> Result<Option<f64>, IntelError> {
    match rule.kind.as_str() {
        "metric" | "error_rate" | "p95" if sample.eval_metrics => {
            let key = default_metric_key(&rule.kind, &rule.params.metric_key);
            Ok(metrics.get(&key).copied())
        }
        "log_pattern" if sample.eval_logs => Ok(Some(log_rate(conn, rule, sample)?)),
        "ssl_expiry" | "uptime" => {
            if let Some(probed) = sample.probes.get(&rule.id) {
                state.checked_at_ms = sample.ts_ms;
                return match probed {
                    Ok(value) => Ok(Some(*value)),
                    Err(err) => {
                        state.last_error = Some(err.clone());
                        Ok(None)
                    }
                };
            }
            Ok(state.last_value)
        }
        _ => Ok(None),
    }
}

fn log_rate(conn: &Connection, rule: &IntelRule, sample: &Sample<'_>) -> Result<f64, IntelError> {
    if rule.params.pattern.is_empty() {
        return Err(IntelError::Bad("log pattern is empty".into()));
    }
    let regex = Regex::new(&rule.params.pattern).map_err(|e| IntelError::Bad(e.to_string()))?;
    let window = rule.params.window_secs.max(1) as i64 * 1000;
    let since = sample.ts_ms - window;
    for line in sample.logs.iter().take(400) {
        if line.ts_ms < since {
            continue;
        }
        let text = clip_chars(&line.text, 2000);
        if regex.is_match(&text) {
            let id = clip_chars(&line.id, 80);
            store::remember_log_hit(conn, &rule.id, &id, line.ts_ms.max(since))?;
        }
    }
    store::prune_log_hits(conn, &rule.id, since)?;
    let count = store::count_log_hits(conn, &rule.id, since)?;
    Ok(log_rate_per_min(count, rule.params.window_secs))
}

fn alert_message(rule: &IntelRule, server_name: &str, value: f64) -> String {
    let detail = match rule.kind.as_str() {
        "log_pattern" => format!(
            "pattern /{}/ at {:.2}/min (threshold {:.2})",
            rule.params.pattern, value, rule.params.threshold
        ),
        "ssl_expiry" => format!(
            "certificate for {}:{} has {:.1} days left (threshold {:.0})",
            rule.params.host, rule.params.port, value, rule.params.threshold
        ),
        "uptime" => format!(
            "uptime check {}:{} is {}",
            rule.params.host,
            rule.params.port,
            if value >= 1.0 { "up" } else { "down" }
        ),
        _ => {
            let key = default_metric_key(&rule.kind, &rule.params.metric_key);
            let sense = if is_low_kind(&rule.kind) { "at or below" } else { "at or above" };
            format!(
                "{key} is {value:.2}, {sense} {:.2}",
                rule.params.threshold
            )
        }
    };
    format!("{server_name}: {detail}")
}

fn attach_alert(
    conn: &Connection,
    sample: &Sample<'_>,
    severity: &str,
    title: &str,
    message: &str,
) -> Result<(), IntelError> {
    let id = store::open_incident(
        conn,
        sample.server_id,
        &clip_chars(title, 120),
        severity,
        sample.ts_ms,
    )?;
    store::add_timeline(conn, &id, sample.ts_ms, "alert", message)?;
    store::touch_incident(conn, &id, sample.ts_ms)?;
    Ok(())
}

fn observe_metric(conn: &Connection, sample: &Sample<'_>, key: &str, value: f64) -> Result<(), IntelError> {
    if !value.is_finite() {
        return Ok(());
    }
    let slot = hour_of_week(sample.ts_ms);
    let mut model = store::load_baseline(conn, sample.server_id, key, slot)?;
    let stepped = step_model(&mut model, value);
    store::save_baseline(conn, sample.server_id, key, slot, &model)?;
    if !stepped.anomalous {
        return Ok(());
    }
    if let Some(previous) = store::latest_anomaly_ts(conn, sample.server_id, key)? {
        if sample.ts_ms.saturating_sub(previous) < 300_000 {
            return Ok(());
        }
    }
    let message = format!(
        "{key} is {:.2} against a baseline of {:.2} (robust z {:.1})",
        value, stepped.baseline, stepped.z
    );
    store::add_anomaly(
        conn,
        &AnomalyEvent {
            id: Uuid::new_v4().to_string(),
            server_id: sample.server_id.into(),
            metric_key: key.into(),
            ts_ms: sample.ts_ms,
            value,
            baseline: stepped.baseline,
            z: stepped.z,
            message: message.clone(),
        },
    )?;
    Ok(())
}

fn observe_restarts(conn: &Connection, sample: &Sample<'_>) -> Result<(), IntelError> {
    for app in sample.restarts.iter().take(200) {
        let previous = store::get_restart(conn, sample.server_id, &app.id)?;
        if let Some((from, to)) = restart_increase(previous, app.restarts) {
            let message = format!("PM2 restart: {} count {from} → {to}", app.name);
            store::add_feed(conn, sample.server_id, sample.ts_ms, "deploy", &message)?;
            if let Some(open) = store::list_incidents(conn, Some(sample.server_id))?
                .into_iter()
                .find(|incident| incident.status != "resolved")
            {
                store::add_timeline(conn, &open.id, sample.ts_ms, "deploy", &message)?;
                store::touch_incident(conn, &open.id, sample.ts_ms)?;
            }
        }
        store::set_restart(conn, sample.server_id, &app.id, &app.name, app.restarts)?;
    }
    Ok(())
}

fn fold_alerts(conn: &Connection, sample: &Sample<'_>) -> Result<Vec<Notice>, IntelError> {
    let rows = store::recent_open_alerts(conn, sample.server_id, sample.ts_ms - 15 * 60 * 1000)?;
    let mut notices = Vec::new();
    for (id, name, message, ts) in rows {
        if !store::claim_alert(conn, &id, ts)? {
            continue;
        }
        let incident = store::open_incident(conn, sample.server_id, &clip_chars(&name, 120), "critical", ts)?;
        store::add_timeline(conn, &incident, ts, "alert", &message)?;
        store::touch_incident(conn, &incident, sample.ts_ms)?;
        notices.push(Notice {
            title: format!("Tracelyn: {name}"),
            body: message,
        });
    }
    Ok(notices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alerts::{migrate_rules, AlertHistoryStore};
    use crate::intel::store;
    use crate::intel::types::{IntelRule, RuleParams};

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        store::migrate(&conn).unwrap();
        conn
    }

    fn rule(kind: &str, threshold: f64, duration: u64, extra: RuleParams) -> IntelRule {
        let mut params = extra;
        params.threshold = threshold;
        params.duration_secs = duration;
        IntelRule {
            id: format!("rule-{kind}"),
            server_id: None,
            kind: kind.into(),
            name: kind.into(),
            enabled: true,
            severity: "warning".into(),
            params,
            created_at_ms: 0,
            updated_at_ms: 0,
            firing: false,
        }
    }

    fn sample<'a>(
        metrics: &'a HashMap<String, f64>,
        logs: &'a [LogObs],
        restarts: &'a [RestartObs],
        probes: &'a HashMap<String, Result<f64, String>>,
        ts: i64,
    ) -> Sample<'a> {
        Sample {
            server_id: "srv",
            server_name: "Web",
            ts_ms: ts,
            metrics,
            logs,
            restarts,
            probes,
            eval_metrics: true,
            eval_logs: true,
            eval_anomaly: true,
        }
    }

    #[test]
    fn metric_rule_respects_duration_and_hysteresis() {
        let conn = conn();
        let mut params = RuleParams::default();
        params.metric_key = "cpu.percent".into();
        params.clear_threshold = Some(50.0);
        params.clear_duration_secs = 30;
        store::upsert_rule(&conn, &rule("metric", 80.0, 60, params)).unwrap();
        let mut metrics = HashMap::new();
        metrics.insert("cpu.percent".into(), 90.0);
        let empty_logs = [];
        let empty_restarts = [];
        let empty_probes = HashMap::new();
        let first = run(
            &conn,
            &sample(&metrics, &empty_logs, &empty_restarts, &empty_probes, 0),
        )
        .unwrap();
        assert_eq!(first.fired, 0);
        let second = run(
            &conn,
            &sample(&metrics, &empty_logs, &empty_restarts, &empty_probes, 60_000),
        )
        .unwrap();
        assert_eq!(second.fired, 1);
        metrics.insert("cpu.percent".into(), 40.0);
        let cooling = run(
            &conn,
            &sample(&metrics, &empty_logs, &empty_restarts, &empty_probes, 70_000),
        )
        .unwrap();
        assert_eq!(cooling.cleared, 0);
        let cleared = run(
            &conn,
            &sample(&metrics, &empty_logs, &empty_restarts, &empty_probes, 100_000),
        )
        .unwrap();
        assert_eq!(cleared.cleared, 1);
        let incidents = store::list_incidents(&conn, Some("srv")).unwrap();
        assert_eq!(incidents.len(), 1);
        let timeline = store::list_timeline(&conn, &incidents[0].id).unwrap();
        assert!(timeline.len() >= 2);
    }

    #[test]
    fn log_pattern_dedupes_line_ids() {
        let conn = conn();
        let mut params = RuleParams::default();
        params.pattern = "ERROR".into();
        params.window_secs = 60;
        params.clear_duration_secs = 30;
        store::upsert_rule(&conn, &rule("log_pattern", 1.0, 0, params)).unwrap();
        let metrics = HashMap::new();
        let logs = [LogObs {
            id: "line-1".into(),
            ts_ms: 1_000,
            text: "ERROR disk".into(),
        }];
        let probes = HashMap::new();
        let restarts = [];
        let first = run(&conn, &sample(&metrics, &logs, &restarts, &probes, 1_000)).unwrap();
        assert_eq!(first.fired, 1);
        let second = run(&conn, &sample(&metrics, &logs, &restarts, &probes, 2_000)).unwrap();
        assert_eq!(second.fired, 0, "the same line must not raise the rate again");
    }

    #[test]
    fn uptime_down_then_up() {
        let conn = conn();
        let mut params = RuleParams::default();
        params.host = "10.0.0.5".into();
        params.port = 443;
        params.clear_duration_secs = 0;
        let mut spec = rule("uptime", 0.0, 0, params);
        spec.severity = "critical".into();
        store::upsert_rule(&conn, &spec).unwrap();
        let metrics = HashMap::new();
        let logs = [];
        let restarts = [];
        let mut probes = HashMap::new();
        probes.insert(spec.id.clone(), Ok(0.0));
        let fired = run(&conn, &sample(&metrics, &logs, &restarts, &probes, 5_000)).unwrap();
        assert_eq!(fired.fired, 1);
        probes.insert(spec.id, Ok(1.0));
        let cleared = run(&conn, &sample(&metrics, &logs, &restarts, &probes, 6_000)).unwrap();
        assert_eq!(cleared.cleared, 1);
    }

    #[test]
    fn ssl_uses_low_threshold() {
        let conn = conn();
        let mut params = RuleParams::default();
        params.host = "example.com".into();
        params.port = 443;
        params.clear_threshold = Some(21.0);
        params.clear_duration_secs = 0;
        let spec = rule("ssl_expiry", 14.0, 0, params);
        store::upsert_rule(&conn, &spec).unwrap();
        let metrics = HashMap::new();
        let mut probes = HashMap::new();
        probes.insert(spec.id.clone(), Ok(10.0));
        let fired = run(&conn, &sample(&metrics, &[], &[], &probes, 1)).unwrap();
        assert_eq!(fired.fired, 1);
        probes.insert(spec.id, Ok(30.0));
        let cleared = run(&conn, &sample(&metrics, &[], &[], &probes, 2)).unwrap();
        assert_eq!(cleared.cleared, 1);
    }

    #[test]
    fn restarts_record_a_deploy_after_the_first_sighting() {
        let conn = conn();
        let metrics = HashMap::new();
        let probes = HashMap::new();
        let first = [RestartObs {
            id: "app".into(),
            name: "api".into(),
            restarts: 3,
        }];
        run(&conn, &sample(&metrics, &[], &first, &probes, 1)).unwrap();
        assert!(store::list_feed_between(&conn, "srv", 0, 10).unwrap().is_empty());
        let second = [RestartObs {
            id: "app".into(),
            name: "api".into(),
            restarts: 5,
        }];
        run(&conn, &sample(&metrics, &[], &second, &probes, 2)).unwrap();
        let feed = store::list_feed_between(&conn, "srv", 0, 10).unwrap();
        assert_eq!(feed.len(), 1);
        assert_eq!(feed[0].kind, "deploy");
        assert!(feed[0].message.contains("3 → 5"));
    }

    #[test]
    fn existing_alert_events_join_one_incident() {
        let conn = conn();
        migrate_rules(&conn).unwrap();
        AlertHistoryStore::migrate(&conn).unwrap();
        AlertHistoryStore::insert_firing(&conn, "r", "CPU", "srv", None, "cpu hot", 95.0, 90.0, 1_000).unwrap();
        AlertHistoryStore::insert_firing(&conn, "r2", "Disk", "srv", None, "disk hot", 91.0, 85.0, 2_000).unwrap();
        let metrics = HashMap::new();
        let probes = HashMap::new();
        run(&conn, &sample(&metrics, &[], &[], &probes, 3_000)).unwrap();
        run(&conn, &sample(&metrics, &[], &[], &probes, 4_000)).unwrap();
        let incidents = store::list_incidents(&conn, Some("srv")).unwrap();
        assert_eq!(incidents.len(), 1);
        let timeline = store::list_timeline(&conn, &incidents[0].id).unwrap();
        assert_eq!(timeline.len(), 2);
    }

    #[test]
    fn anomaly_spike_is_stored_once_inside_five_minutes() {
        let conn = conn();
        let probes = HashMap::new();
        let mut metrics = HashMap::new();
        metrics.insert("cpu.percent".into(), 10.0);
        for i in 0..10 {
            run(
                &conn,
                &sample(&metrics, &[], &[], &probes, i * 60_000),
            )
            .unwrap();
        }
        metrics.insert("cpu.percent".into(), 90.0);
        run(&conn, &sample(&metrics, &[], &[], &probes, 700_000)).unwrap();
        run(&conn, &sample(&metrics, &[], &[], &probes, 760_000)).unwrap();
        let events = store::list_anomalies(&conn, "srv", 0, 2_000_000).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].message.contains("cpu.percent"));
    }

    #[test]
    fn ack_note_and_resolve() {
        let conn = conn();
        let id = store::open_incident(&conn, "srv", "CPU", "warning", 10).unwrap();
        store::set_incident_status(&conn, &id, "ack", 20).unwrap();
        store::add_timeline(&conn, &id, 30, "note", "checking disk").unwrap();
        store::set_incident_status(&conn, &id, "resolved", 40).unwrap();
        store::prune(&conn).unwrap();
        let incident = store::get_incident(&conn, &id).unwrap().unwrap();
        assert_eq!(incident.status, "resolved");
        assert_eq!(incident.acked_at_ms, Some(20));
        let timeline = store::list_timeline(&conn, &id).unwrap();
        assert_eq!(timeline[0].kind, "note");
    }
}

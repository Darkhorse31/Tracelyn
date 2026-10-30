use super::history::AlertHistoryStore;
use super::log_query::{count_matches, parse_log_query, LogSample};
use super::rules::{list_rules, AlertRule, RuleKind};
use crate::intel::policy::{cooldown_elapsed, in_quiet_hours, minute_of_day};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone)]
pub struct EvaluationInput {
    pub server_id: String,
    pub server_name: String,
    pub ts_ms: i64,
    pub metrics: HashMap<String, f64>,
    pub connected: bool,
    pub utc_offset_secs: i32,
    pub logs: Vec<LogSample>,
    /// False when the log stream has not reported yet. A log rule is then no-data.
    pub logs_available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertTransition {
    pub rule_id: String,
    pub rule_name: String,
    pub server_id: String,
    pub fired: bool,
    pub message: String,
    pub value: f64,
    pub threshold: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationOutcome {
    pub transitions: Vec<AlertTransition>,
}

pub struct AlertEngine;

impl AlertEngine {
    pub fn evaluate(
        conn: &Connection,
        input: &EvaluationInput,
    ) -> Result<EvaluationOutcome, EngineError> {
        let rules = list_rules(conn).map_err(|e| match e {
            super::rules::RuleError::Sqlite(err) => EngineError::Sqlite(err),
        })?;
        let mut transitions = Vec::new();

        for rule in rules {
            if crate::companion_history::owns_basic(&input.server_id, &rule) {
                continue;
            }
            if !rule.enabled {
                continue;
            }
            if let Some(scope) = &rule.server_id {
                if scope != &input.server_id {
                    continue;
                }
            }

            let signal = rule_signal(&rule, input);
            let clear_threshold = rule
                .params
                .clear_threshold
                .unwrap_or(rule.params.threshold * 0.95);

            let mut state = load_state(conn, &rule.id, &input.server_id)?;
            let was_firing = state.firing;
            let notified_at = state.last_notify_ms;
            if state.no_data && !signal.no_data && !state.firing {
                state.condition_since_ms = None;
            }
            let new_state = if signal.no_data {
                advance_no_data(
                    state,
                    input.ts_ms,
                    rule.params.duration_secs,
                    rule.params.alert_on_no_data,
                )
            } else {
                advance_state(
                    state,
                    signal.condition,
                    signal.value,
                    input.ts_ms,
                    rule.params.duration_secs,
                    clear_threshold,
                    rule.params.threshold,
                )
            };
            let mut new_state = new_state;
            new_state.no_data = signal.no_data;

            let episode_since = new_state.condition_since_ms.unwrap_or(input.ts_ms);
            let needs_notify = new_state.firing
                && new_state
                    .last_notify_ms
                    .map(|notified| notified < episode_since)
                    .unwrap_or(true);
            if needs_notify {
                let detail = if signal.no_data {
                    format!(
                        "No data for {}",
                        signal.metric_key.as_deref().unwrap_or("metric")
                    )
                } else {
                    format_alert_message(&rule, signal.value, signal.metric_key.as_deref())
                };
                let message = format!(
                    "{}: {}. Since {}",
                    input.server_name.replace(';', ","),
                    detail.replace(';', ","),
                    chrono::DateTime::from_timestamp_millis(episode_since)
                        .map(|v| v.to_rfc3339())
                        .unwrap_or_else(|| episode_since.to_string())
                );
                let fired_at = episode_since;
                let recorded: i64 = conn.query_row("SELECT COUNT(*) FROM alert_events WHERE rule_id=?1 AND server_id=?2 AND fired_at_ms=?3", params![rule.id,input.server_id,fired_at], |r|r.get(0))?;
                if recorded == 0 {
                    AlertHistoryStore::insert_firing(
                        conn,
                        &rule.id,
                        &rule.name,
                        &input.server_id,
                        signal.metric_key.as_deref(),
                        &message,
                        signal.value,
                        rule.params.threshold,
                        fired_at,
                    )
                    .map_err(history_err)?;
                }
                let _ = AlertHistoryStore::record_marker(
                    conn,
                    &input.server_id,
                    fired_at,
                    "alert",
                    &rule.name,
                    &message,
                );
                if allow_notify(&rule, input, notified_at) {
                    new_state.last_notify_ms = Some(input.ts_ms);
                    transitions.push(AlertTransition {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        server_id: input.server_id.clone(),
                        fired: true,
                        message,
                        value: signal.value,
                        threshold: rule.params.threshold,
                    });
                }
            } else if !new_state.firing && was_firing {
                AlertHistoryStore::resolve_open(conn, &rule.id, &input.server_id, input.ts_ms)
                    .map_err(history_err)?;
                transitions.push(AlertTransition {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    server_id: input.server_id.clone(),
                    fired: false,
                    message: format!("{} cleared", rule.name),
                    value: signal.value,
                    threshold: rule.params.threshold,
                });
            }
            save_state(conn, &rule.id, &input.server_id, &new_state)?;
        }

        Ok(EvaluationOutcome { transitions })
    }
}

fn history_err(err: super::history::HistoryError) -> EngineError {
    match err {
        super::history::HistoryError::Sqlite(err) => EngineError::Sqlite(err),
    }
}

#[derive(Debug, Clone, Default)]
struct RuleState {
    firing: bool,
    no_data: bool,
    condition_since_ms: Option<i64>,
    clear_since_ms: Option<i64>,
    last_value: Option<f64>,
    last_notify_ms: Option<i64>,
}

struct Signal {
    condition: bool,
    no_data: bool,
    value: f64,
    metric_key: Option<String>,
}

fn load_state(conn: &Connection, rule_id: &str, server_id: &str) -> Result<RuleState, EngineError> {
    let mut stmt = conn.prepare(
        "SELECT firing, condition_since_ms, clear_since_ms, last_value, no_data, last_notify_ms
         FROM alert_rule_state WHERE rule_id = ?1 AND server_id = ?2",
    )?;
    let mut rows = stmt.query(params![rule_id, server_id])?;
    if let Some(row) = rows.next()? {
        return Ok(RuleState {
            firing: row.get::<_, i64>(0)? != 0,
            condition_since_ms: row.get(1)?,
            clear_since_ms: row.get(2)?,
            last_value: row.get(3)?,
            no_data: row.get::<_, i64>(4)? != 0,
            last_notify_ms: row.get(5)?,
        });
    }
    Ok(RuleState::default())
}

fn save_state(
    conn: &Connection,
    rule_id: &str,
    server_id: &str,
    state: &RuleState,
) -> Result<(), EngineError> {
    conn.execute(
        "INSERT INTO alert_rule_state
            (rule_id, server_id, firing, condition_since_ms, clear_since_ms, last_value, no_data, last_notify_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(rule_id, server_id) DO UPDATE SET
            firing = excluded.firing,
            condition_since_ms = excluded.condition_since_ms,
            clear_since_ms = excluded.clear_since_ms,
            last_value = excluded.last_value,
            no_data = excluded.no_data,
            last_notify_ms = excluded.last_notify_ms",
        params![
            rule_id,
            server_id,
            if state.firing { 1 } else { 0 },
            state.condition_since_ms,
            state.clear_since_ms,
            state.last_value,
            if state.no_data { 1 } else { 0 },
            state.last_notify_ms
        ],
    )?;
    Ok(())
}

fn advance_no_data(
    prev: RuleState,
    ts_ms: i64,
    duration_secs: u64,
    alert_on_no_data: bool,
) -> RuleState {
    let duration_ms = duration_secs as i64 * 1000;
    let mut next = prev;
    let since = if next.no_data || next.firing {
        next.condition_since_ms.unwrap_or(ts_ms)
    } else {
        ts_ms
    };
    next.condition_since_ms = Some(since);
    next.no_data = true;
    next.last_value = None;
    let held = duration_ms == 0 || ts_ms - since >= duration_ms;
    if !held {
        return next;
    }
    if alert_on_no_data {
        next.firing = true;
        next.clear_since_ms = None;
    } else if next.firing {
        next.firing = false;
        next.clear_since_ms = None;
    }
    next
}

fn allow_notify(rule: &AlertRule, input: &EvaluationInput, last_notify_ms: Option<i64>) -> bool {
    if rule
        .params
        .silence_until_ms
        .is_some_and(|until| until > input.ts_ms)
    {
        return false;
    }
    if let (Some(start), Some(end)) = (rule.params.quiet_start_min, rule.params.quiet_end_min) {
        let minute = minute_of_day(input.ts_ms, input.utc_offset_secs);
        if in_quiet_hours(minute, start, end) {
            return false;
        }
    }
    cooldown_elapsed(
        last_notify_ms,
        input.ts_ms,
        rule.params.cooldown_secs as i64 * 1000,
    )
}

fn advance_state(
    prev: RuleState,
    condition: bool,
    value: f64,
    ts_ms: i64,
    duration_secs: u64,
    clear_threshold: f64,
    fire_threshold: f64,
) -> RuleState {
    let duration_ms = duration_secs as i64 * 1000;
    let mut next = prev;
    next.last_value = Some(value);

    if !next.firing {
        if condition {
            let since = next.condition_since_ms.unwrap_or(ts_ms);
            next.condition_since_ms = Some(since);
            if duration_ms == 0 || ts_ms - since >= duration_ms {
                next.firing = true;
                next.clear_since_ms = None;
            }
        } else {
            next.condition_since_ms = None;
        }
        return next;
    }

    // Firing: clear when below clear threshold (hysteresis)
    let below_clear = value < clear_threshold || (!condition && value < fire_threshold);
    if below_clear {
        let since = next.clear_since_ms.unwrap_or(ts_ms);
        next.clear_since_ms = Some(since);
        if ts_ms - since >= 30_000 {
            next.firing = false;
            next.condition_since_ms = None;
            next.clear_since_ms = None;
        }
    } else {
        next.clear_since_ms = None;
    }
    next
}

fn rule_signal(rule: &AlertRule, input: &EvaluationInput) -> Signal {
    if !rule.params.conditions.is_empty()
        && !matches!(
            rule.kind,
            RuleKind::ServerUnreachable
                | RuleKind::LogQuery
                | RuleKind::TraceP95
                | RuleKind::TraceErrorRate
        )
    {
        return multi_signal(rule, input);
    }
    match rule.kind {
        RuleKind::ServerUnreachable => {
            let unreachable = !input.connected;
            Signal {
                condition: unreachable,
                no_data: false,
                value: if unreachable { 1.0 } else { 0.0 },
                metric_key: None,
            }
        }
        RuleKind::CpuHigh => metric_signal(rule, input, "cpu.percent"),
        RuleKind::MemoryHigh => metric_signal(rule, input, "memory.percent"),
        RuleKind::DiskHigh => metric_signal(rule, input, "disk.percent"),
        RuleKind::Http5xxRate => metric_signal(rule, input, "http.5xx_rate_percent"),
        RuleKind::EndpointP95 => {
            let key = rule
                .params
                .metric_key
                .clone()
                .unwrap_or_else(|| "endpoint.p95_ms".into());
            metric_signal(rule, input, &key)
        }
        RuleKind::RedisMemoryNearMax => metric_signal(rule, input, "redis.memory_used_percent"),
        RuleKind::LogQuery => log_signal(rule, input),
        RuleKind::TraceP95 => {
            let key = trace_key(rule, "p95_ms", "endpoint.p95_ms");
            metric_signal(rule, input, &key)
        }
        RuleKind::TraceErrorRate => {
            let key = trace_key(rule, "error_rate", "http.5xx_rate_percent");
            metric_signal(rule, input, &key)
        }
    }
}

fn trace_key(rule: &AlertRule, field: &str, fallback: &str) -> String {
    let endpoint = rule
        .params
        .endpoint
        .as_deref()
        .filter(|value| !value.is_empty())
        .or(rule
            .params
            .metric_key
            .as_deref()
            .filter(|value| !value.is_empty()));
    match endpoint {
        Some(endpoint) => format!("trace.{endpoint}.{field}"),
        None => fallback.into(),
    }
}

fn metric_signal(rule: &AlertRule, input: &EvaluationInput, default_key: &str) -> Signal {
    let key = if default_key.starts_with("trace.") {
        default_key
    } else {
        rule.params
            .metric_key
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or(default_key)
    };
    match input.metrics.get(key).copied() {
        Some(value) => Signal {
            condition: value >= rule.params.threshold,
            no_data: false,
            value,
            metric_key: Some(key.to_string()),
        },
        None => Signal {
            condition: false,
            no_data: true,
            value: 0.0,
            metric_key: Some(key.to_string()),
        },
    }
}

fn multi_signal(rule: &AlertRule, input: &EvaluationInput) -> Signal {
    let mut value = 0.0;
    let mut first_key = None;
    for (index, condition) in rule.params.conditions.iter().enumerate() {
        let Some(got) = input.metrics.get(&condition.metric_key).copied() else {
            return Signal {
                condition: false,
                no_data: true,
                value,
                metric_key: Some(condition.metric_key.clone()),
            };
        };
        if index == 0 {
            value = got;
            first_key = Some(condition.metric_key.clone());
        }
        if !compare(&condition.op, got, condition.threshold) {
            return Signal {
                condition: false,
                no_data: false,
                value: got,
                metric_key: first_key,
            };
        }
    }
    Signal {
        condition: true,
        no_data: false,
        value,
        metric_key: first_key,
    }
}

fn log_signal(rule: &AlertRule, input: &EvaluationInput) -> Signal {
    let query = rule.params.log_query.clone().unwrap_or_default();
    if !input.logs_available {
        return Signal {
            condition: false,
            no_data: true,
            value: 0.0,
            metric_key: Some(query),
        };
    }
    let parsed = parse_log_query(&query);
    let since = input.ts_ms - rule.params.window_secs.max(1) as i64 * 1000;
    let count = count_matches(&parsed, &input.logs, since) as f64;
    Signal {
        condition: count >= rule.params.threshold,
        no_data: false,
        value: count,
        metric_key: Some(query),
    }
}

fn compare(op: &str, value: f64, threshold: f64) -> bool {
    match op {
        "lte" | "<=" => value <= threshold,
        "lt" | "<" => value < threshold,
        "gt" | ">" => value > threshold,
        "eq" | "=" => (value - threshold).abs() < 1e-6,
        _ => value >= threshold,
    }
}

fn format_alert_message(rule: &AlertRule, value: f64, metric_key: Option<&str>) -> String {
    match rule.kind {
        RuleKind::ServerUnreachable => format!("The server connection has been unavailable for over {} seconds. Check the server and its network", rule.params.duration_secs),
        RuleKind::LogQuery => format!(
            "Log query matched {:.0} lines in {}s (threshold {:.0})",
            value,
            rule.params.window_secs,
            rule.params.threshold
        ),
        RuleKind::TraceP95 => format!(
            "Endpoint p95 latency {:.0} ms exceeds {:.0} ms",
            value,
            rule.params.threshold
        ),
        RuleKind::TraceErrorRate => format!(
            "Endpoint error rate {:.1}% exceeds {:.1}%",
            value,
            rule.params.threshold
        ),
        RuleKind::EndpointP95 => format!(
            "Endpoint p95 latency {:.0} ms exceeds {:.0} ms threshold",
            value,
            rule.params.threshold
        ),
        _ => {
            let label = metric_key.unwrap_or("metric");
            format!(
                "{}: {:.1} exceeds threshold {:.1}",
                label,
                value,
                rule.params.threshold
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alerts::defaults::seed_default_rules;
    use crate::alerts::log_query::LogSample;
    use crate::alerts::rules::{migrate_rules, upsert_rule, AlertRule, RuleParams};

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("conn");
        migrate_rules(&conn).expect("migrate rules");
        AlertHistoryStore::migrate(&conn).expect("migrate history");
        seed_default_rules(&conn, 1_700_000_000_000).expect("seed");
        conn
    }

    #[test]
    fn cpu_rule_requires_duration_before_firing() {
        let conn = setup_conn();
        let rules = list_rules(&conn).expect("rules");
        let cpu = rules
            .iter()
            .find(|r| r.kind == RuleKind::CpuHigh)
            .expect("cpu rule");

        let base = 1_700_000_000_000i64;
        let mut metrics = HashMap::new();
        metrics.insert("cpu.percent".into(), 95.0);

        let input = EvaluationInput {
            server_id: "srv-1".into(),
            server_name: "Production".into(),
            ts_ms: base,
            metrics: metrics.clone(),
            connected: true,
            utc_offset_secs: 0,
            logs: Vec::new(),
            logs_available: false,
        };
        let out = AlertEngine::evaluate(&conn, &input).expect("eval");
        assert!(out.transitions.is_empty());

        let input2 = EvaluationInput {
            server_id: "srv-1".into(),
            server_name: "Production".into(),
            ts_ms: base + 301_000,
            metrics,
            connected: true,
            utc_offset_secs: 0,
            logs: Vec::new(),
            logs_available: false,
        };
        let out2 = AlertEngine::evaluate(&conn, &input2).expect("eval2");
        assert_eq!(out2.transitions.len(), 1);
        assert!(out2.transitions[0].fired);
        assert_eq!(out2.transitions[0].rule_id, cpu.id);
    }

    #[test]
    fn hysteresis_clears_after_value_drops() {
        let conn = setup_conn();
        let rules = list_rules(&conn).expect("rules");
        let disk = rules
            .iter()
            .find(|r| r.kind == RuleKind::DiskHigh)
            .expect("disk");

        let base = 1_700_000_000_000i64;
        let mut metrics = HashMap::new();
        metrics.insert("disk.percent".into(), 90.0);

        AlertEngine::evaluate(
            &conn,
            &EvaluationInput {
                server_id: "srv-1".into(),
                server_name: "Production".into(),
                ts_ms: base,
                metrics: metrics.clone(),
                connected: true,
                utc_offset_secs: 0,
                logs: Vec::new(),
                logs_available: false,
            },
        )
        .expect("fire");

        metrics.insert("disk.percent".into(), 75.0);
        AlertEngine::evaluate(
            &conn,
            &EvaluationInput {
                server_id: "srv-1".into(),
                server_name: "Production".into(),
                ts_ms: base + 5_000,
                metrics: metrics.clone(),
                connected: true,
                utc_offset_secs: 0,
                logs: Vec::new(),
                logs_available: false,
            },
        )
        .expect("below clear threshold");

        let cleared = AlertEngine::evaluate(
            &conn,
            &EvaluationInput {
                server_id: "srv-1".into(),
                server_name: "Production".into(),
                ts_ms: base + 40_000,
                metrics,
                connected: true,
                utc_offset_secs: 0,
                logs: Vec::new(),
                logs_available: false,
            },
        )
        .expect("clear");
        assert_eq!(cleared.transitions.len(), 1);
        assert!(!cleared.transitions[0].fired);
        assert_eq!(cleared.transitions[0].rule_id, disk.id);
    }

    fn bare() -> Connection {
        let conn = Connection::open_in_memory().expect("conn");
        migrate_rules(&conn).expect("rules");
        AlertHistoryStore::migrate(&conn).expect("history");
        conn
    }

    #[test]
    fn unreachable_message_names_the_server_and_records_only_one_episode() {
        let conn = bare();
        put(
            &conn,
            RuleKind::ServerUnreachable,
            RuleParams {
                threshold: 1.0,
                ..RuleParams::default()
            },
        );
        let mut input = at(1000, HashMap::new(), vec![], false);
        input.connected = false;
        let first = AlertEngine::evaluate(&conn, &input).unwrap();
        assert_eq!(first.transitions.len(), 1);
        assert!(first.transitions[0].message.contains("Production"));
        assert!(first.transitions[0].message.contains("Since"));
        assert!(!first.transitions[0].message.contains(';'));
        input.ts_ms = 2000;
        assert!(AlertEngine::evaluate(&conn, &input)
            .unwrap()
            .transitions
            .is_empty());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM alert_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    fn put(conn: &Connection, kind: RuleKind, params: RuleParams) {
        upsert_rule(
            conn,
            &AlertRule {
                id: "rule-1".into(),
                server_id: None,
                kind,
                name: "probe".into(),
                enabled: true,
                params,
                created_at_ms: 0,
                updated_at_ms: 0,
                firing: false,
                no_data: false,
            },
        )
        .unwrap();
    }

    fn at(
        ts: i64,
        metrics: HashMap<String, f64>,
        logs: Vec<LogSample>,
        logs_available: bool,
    ) -> EvaluationInput {
        EvaluationInput {
            server_id: "srv-1".into(),
            server_name: "Production".into(),
            ts_ms: ts,
            metrics,
            connected: true,
            utc_offset_secs: 0,
            logs,
            logs_available,
        }
    }

    fn reset(conn: &Connection) {
        conn.execute("DELETE FROM alert_rule_state", []).unwrap();
        conn.execute("DELETE FROM alert_events", []).unwrap();
    }

    #[test]
    fn multi_condition_waits_for_every_metric() {
        let conn = bare();
        put(
            &conn,
            RuleKind::CpuHigh,
            RuleParams {
                conditions: vec![
                    crate::alerts::rules::RuleCondition {
                        metric_key: "cpu.percent".into(),
                        op: "gte".into(),
                        threshold: 80.0,
                    },
                    crate::alerts::rules::RuleCondition {
                        metric_key: "memory.percent".into(),
                        op: "gte".into(),
                        threshold: 70.0,
                    },
                ],
                ..RuleParams::default()
            },
        );
        let mut metrics = HashMap::new();
        metrics.insert("cpu.percent".into(), 90.0);
        let missing =
            AlertEngine::evaluate(&conn, &at(1_000, metrics.clone(), vec![], false)).unwrap();
        assert!(missing.transitions.is_empty());
        assert!(list_rules(&conn).unwrap()[0].no_data);
        metrics.insert("memory.percent".into(), 75.0);
        let fired = AlertEngine::evaluate(&conn, &at(2_000, metrics, vec![], false)).unwrap();
        assert_eq!(fired.transitions.len(), 1);
        assert!(fired.transitions[0].fired);
    }

    #[test]
    fn missing_metric_is_no_data_and_can_fire() {
        let conn = bare();
        put(
            &conn,
            RuleKind::MemoryHigh,
            RuleParams {
                threshold: 90.0,
                alert_on_no_data: true,
                ..RuleParams::default()
            },
        );
        let out = AlertEngine::evaluate(&conn, &at(1_000, HashMap::new(), vec![], false)).unwrap();
        assert!(out
            .transitions
            .iter()
            .any(|t| t.fired && t.message.contains("No data")));
    }

    #[test]
    fn cooldown_and_quiet_hours_hold_the_notification() {
        let conn = bare();
        put(
            &conn,
            RuleKind::CpuHigh,
            RuleParams {
                threshold: 80.0,
                clear_threshold: Some(10.0),
                cooldown_secs: 600,
                metric_key: Some("cpu.percent".into()),
                ..RuleParams::default()
            },
        );
        let mut hot = HashMap::new();
        hot.insert("cpu.percent".into(), 90.0);
        assert_eq!(
            AlertEngine::evaluate(&conn, &at(0, hot.clone(), vec![], false))
                .unwrap()
                .transitions
                .len(),
            1
        );
        hot.insert("cpu.percent".into(), 1.0);
        AlertEngine::evaluate(&conn, &at(5_000, hot.clone(), vec![], false)).unwrap();
        let cleared =
            AlertEngine::evaluate(&conn, &at(40_000, hot.clone(), vec![], false)).unwrap();
        assert!(cleared.transitions.iter().any(|t| !t.fired));
        hot.insert("cpu.percent".into(), 90.0);
        let blocked =
            AlertEngine::evaluate(&conn, &at(50_000, hot.clone(), vec![], false)).unwrap();
        assert!(blocked.transitions.iter().all(|t| !t.fired));

        reset(&conn);
        put(
            &conn,
            RuleKind::CpuHigh,
            RuleParams {
                threshold: 80.0,
                metric_key: Some("cpu.percent".into()),
                quiet_start_min: Some(0),
                quiet_end_min: Some(60),
                ..RuleParams::default()
            },
        );
        let quiet = AlertEngine::evaluate(&conn, &at(30 * 60 * 1000, hot, vec![], false)).unwrap();
        assert!(quiet.transitions.is_empty());
    }

    #[test]
    fn log_query_and_trace_p95_fire() {
        let conn = bare();
        put(
            &conn,
            RuleKind::LogQuery,
            RuleParams {
                threshold: 1.0,
                log_query: Some("level:error payment".into()),
                window_secs: 60,
                ..RuleParams::default()
            },
        );
        let logs = vec![LogSample {
            raw: "payment failed".into(),
            level: "error".into(),
            message: "payment failed".into(),
            app: "api".into(),
            source: "stdout".into(),
            status: Some(500.0),
            trace_id: String::new(),
            span_id: String::new(),
            ts_ms: 5_000,
        }];
        let logged = AlertEngine::evaluate(&conn, &at(5_000, HashMap::new(), logs, true)).unwrap();
        assert!(logged
            .transitions
            .iter()
            .any(|t| t.message.contains("Log query")));

        reset(&conn);
        put(
            &conn,
            RuleKind::TraceErrorRate,
            RuleParams {
                threshold: 5.0,
                endpoint: Some("GET /pay".into()),
                ..RuleParams::default()
            },
        );
        let mut traces = HashMap::new();
        traces.insert("trace.GET /pay.error_rate".into(), 12.0);
        let trace = AlertEngine::evaluate(&conn, &at(6_000, traces, vec![], false)).unwrap();
        assert!(trace
            .transitions
            .iter()
            .any(|t| t.fired && t.message.contains("error rate")));
    }
}

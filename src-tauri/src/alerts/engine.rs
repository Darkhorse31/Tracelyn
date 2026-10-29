use super::history::AlertHistoryStore;
use super::rules::{list_rules, AlertRule, RuleKind};
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
    pub ts_ms: i64,
    pub metrics: HashMap<String, f64>,
    pub connected: bool,
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
    pub fn evaluate(conn: &Connection, input: &EvaluationInput) -> Result<EvaluationOutcome, EngineError> {
        let rules = list_rules(conn).map_err(|e| match e {
            super::rules::RuleError::Sqlite(err) => EngineError::Sqlite(err),
        })?;
        let mut transitions = Vec::new();

        for rule in rules {
            if crate::companion_history::owns_basic(&input.server_id,&rule) {continue;}
            if !rule.enabled {
                continue;
            }
            if let Some(scope) = &rule.server_id {
                if scope != &input.server_id {
                    continue;
                }
            }

            let (condition, value, metric_key) = rule_condition(&rule, input);
            let clear_threshold = rule
                .params
                .clear_threshold
                .unwrap_or(rule.params.threshold * 0.95);

            let state = load_state(conn, &rule.id, &input.server_id)?;
            let was_firing = state.firing;
            let new_state = advance_state(
                state,
                condition,
                value,
                input.ts_ms,
                rule.params.duration_secs,
                clear_threshold,
                rule.params.threshold,
            );
            save_state(conn, &rule.id, &input.server_id, &new_state)?;

            if new_state.firing && !was_firing {
                let message = format_alert_message(&rule, value, metric_key.as_deref());
                let _event_id = AlertHistoryStore::insert_firing(
                    conn,
                    &rule.id,
                    &rule.name,
                    &input.server_id,
                    metric_key.as_deref(),
                    &message,
                    value,
                    rule.params.threshold,
                    input.ts_ms,
                )
                .map_err(|e| match e {
                    super::history::HistoryError::Sqlite(err) => EngineError::Sqlite(err),
                })?;
                transitions.push(AlertTransition {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    server_id: input.server_id.clone(),
                    fired: true,
                    message,
                    value,
                    threshold: rule.params.threshold,
                });
            } else if !new_state.firing && was_firing {
                AlertHistoryStore::resolve_open(conn, &rule.id, &input.server_id, input.ts_ms)
                    .map_err(|e| match e {
                        super::history::HistoryError::Sqlite(err) => EngineError::Sqlite(err),
                    })?;
                transitions.push(AlertTransition {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    server_id: input.server_id.clone(),
                    fired: false,
                    message: format!("{} cleared", rule.name),
                    value,
                    threshold: rule.params.threshold,
                });
            }
        }

        Ok(EvaluationOutcome { transitions })
    }
}

#[derive(Debug, Clone, Default)]
struct RuleState {
    firing: bool,
    condition_since_ms: Option<i64>,
    clear_since_ms: Option<i64>,
    last_value: Option<f64>,
}

fn load_state(conn: &Connection, rule_id: &str, server_id: &str) -> Result<RuleState, EngineError> {
    let mut stmt = conn.prepare(
        "SELECT firing, condition_since_ms, clear_since_ms, last_value
         FROM alert_rule_state WHERE rule_id = ?1 AND server_id = ?2",
    )?;
    let mut rows = stmt.query(params![rule_id, server_id])?;
    if let Some(row) = rows.next()? {
        return Ok(RuleState {
            firing: row.get::<_, i64>(0)? != 0,
            condition_since_ms: row.get(1)?,
            clear_since_ms: row.get(2)?,
            last_value: row.get(3)?,
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
        "INSERT INTO alert_rule_state (rule_id, server_id, firing, condition_since_ms, clear_since_ms, last_value)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(rule_id, server_id) DO UPDATE SET
            firing = excluded.firing,
            condition_since_ms = excluded.condition_since_ms,
            clear_since_ms = excluded.clear_since_ms,
            last_value = excluded.last_value",
        params![
            rule_id,
            server_id,
            if state.firing { 1 } else { 0 },
            state.condition_since_ms,
            state.clear_since_ms,
            state.last_value
        ],
    )?;
    Ok(())
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

fn rule_condition(rule: &AlertRule, input: &EvaluationInput) -> (bool, f64, Option<String>) {
    match rule.kind {
        RuleKind::ServerUnreachable => {
            let unreachable = !input.connected;
            (unreachable, if unreachable { 1.0 } else { 0.0 }, None)
        }
        RuleKind::CpuHigh => metric_threshold(rule, input, "cpu.percent"),
        RuleKind::MemoryHigh => metric_threshold(rule, input, "memory.percent"),
        RuleKind::DiskHigh => metric_threshold(rule, input, "disk.percent"),
        RuleKind::Http5xxRate => metric_threshold(rule, input, "http.5xx_rate_percent"),
        RuleKind::EndpointP95 => {
            let key = rule
                .params
                .metric_key
                .clone()
                .unwrap_or_else(|| "endpoint.p95_ms".into());
            metric_threshold(rule, input, &key)
        }
        RuleKind::RedisMemoryNearMax => {
            metric_threshold(rule, input, "redis.memory_used_percent")
        }
    }
}

fn metric_threshold(
    rule: &AlertRule,
    input: &EvaluationInput,
    default_key: &str,
) -> (bool, f64, Option<String>) {
    let key = rule
        .params
        .metric_key
        .as_deref()
        .unwrap_or(default_key);
    let value = input.metrics.get(key).copied().unwrap_or(0.0);
    (value >= rule.params.threshold, value, Some(key.to_string()))
}

fn format_alert_message(rule: &AlertRule, value: f64, metric_key: Option<&str>) -> String {
    match rule.kind {
        RuleKind::ServerUnreachable => format!("Server has been unreachable for over {}s", rule.params.duration_secs),
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
    use crate::alerts::rules::migrate_rules;

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
            ts_ms: base,
            metrics: metrics.clone(),
            connected: true,
        };
        let out = AlertEngine::evaluate(&conn, &input).expect("eval");
        assert!(out.transitions.is_empty());

        let input2 = EvaluationInput {
            server_id: "srv-1".into(),
            ts_ms: base + 301_000,
            metrics,
            connected: true,
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
                ts_ms: base,
                metrics: metrics.clone(),
                connected: true,
            },
        )
        .expect("fire");

        metrics.insert("disk.percent".into(), 75.0);
        AlertEngine::evaluate(
            &conn,
            &EvaluationInput {
                server_id: "srv-1".into(),
                ts_ms: base + 5_000,
                metrics: metrics.clone(),
                connected: true,
            },
        )
        .expect("below clear threshold");

        let cleared = AlertEngine::evaluate(
            &conn,
            &EvaluationInput {
                server_id: "srv-1".into(),
                ts_ms: base + 40_000,
                metrics,
                connected: true,
            },
        )
        .expect("clear");
        assert_eq!(cleared.transitions.len(), 1);
        assert!(!cleared.transitions[0].fired);
        assert_eq!(cleared.transitions[0].rule_id, disk.id);
    }
}

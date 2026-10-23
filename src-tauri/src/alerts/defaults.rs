use super::rules::{new_rule_id, upsert_rule, AlertRule, RuleKind, RuleParams, RuleError};
use rusqlite::Connection;

pub fn seed_default_rules(conn: &Connection, now_ms: i64) -> Result<(), RuleError> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM alert_rules", [], |r| r.get(0))?;
    if count > 0 {
        return Ok(());
    }

    let defaults = [
        (
            "Server unreachable",
            RuleKind::ServerUnreachable,
            RuleParams {
                threshold: 1.0,
                clear_threshold: None,
                duration_secs: 60,
                metric_key: None,
                endpoint_label: None,
            },
        ),
        (
            "CPU usage high",
            RuleKind::CpuHigh,
            RuleParams {
                threshold: 90.0,
                clear_threshold: Some(85.0),
                duration_secs: 300,
                metric_key: Some("cpu.percent".into()),
                endpoint_label: None,
            },
        ),
        (
            "Memory usage high",
            RuleKind::MemoryHigh,
            RuleParams {
                threshold: 90.0,
                clear_threshold: Some(85.0),
                duration_secs: 300,
                metric_key: Some("memory.percent".into()),
                endpoint_label: None,
            },
        ),
        (
            "Disk usage high",
            RuleKind::DiskHigh,
            RuleParams {
                threshold: 85.0,
                clear_threshold: Some(80.0),
                duration_secs: 0,
                metric_key: Some("disk.percent".into()),
                endpoint_label: None,
            },
        ),
        (
            "HTTP 5xx rate high",
            RuleKind::Http5xxRate,
            RuleParams {
                threshold: 5.0,
                clear_threshold: Some(3.0),
                duration_secs: 120,
                metric_key: Some("http.5xx_rate_percent".into()),
                endpoint_label: None,
            },
        ),
        (
            "Endpoint p95 latency",
            RuleKind::EndpointP95,
            RuleParams {
                threshold: 500.0,
                clear_threshold: Some(400.0),
                duration_secs: 120,
                metric_key: Some("endpoint.p95_ms".into()),
                endpoint_label: Some("All endpoints".into()),
            },
        ),
        (
            "Redis memory near max",
            RuleKind::RedisMemoryNearMax,
            RuleParams {
                threshold: 90.0,
                clear_threshold: Some(85.0),
                duration_secs: 120,
                metric_key: Some("redis.memory_used_percent".into()),
                endpoint_label: None,
            },
        ),
    ];

    for (name, kind, params) in defaults {
        let rule = AlertRule {
            id: new_rule_id(),
            server_id: None,
            kind,
            name: name.into(),
            enabled: true,
            params,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        };
        upsert_rule(conn, &rule)?;
    }
    Ok(())
}

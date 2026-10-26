use std::time::Duration;

use serde::Serialize;

use super::types::{RedisDbKeyspace, RedisInfo, RedisInfoPayload, RedisSnapshot};
use super::types::DerivedRedisMetricsPayload;

#[derive(Debug, Clone, Default)]
pub struct RateState {
    last_total_commands: Option<u64>,
    last_instantaneous_ops: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedRedisMetrics {
    pub ops_per_sec: f64,
    pub hit_rate_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_peak_bytes: u64,
    pub maxmemory_bytes: u64,
    pub memory_used_ratio: Option<f64>,
    pub connected_clients: u64,
    pub evicted_keys: u64,
    pub expired_keys: u64,
    pub role: String,
    pub total_keys: u64,
}

pub fn derive_metrics(
    info: &RedisInfo,
    keyspace: &[RedisDbKeyspace],
    maxmemory_bytes: u64,
    rate_state: &mut RateState,
    elapsed: Duration,
) -> DerivedRedisMetrics {
    let hits = info.get_u64("keyspace_hits").unwrap_or(0);
    let misses = info.get_u64("keyspace_misses").unwrap_or(0);
    let hit_rate_percent = if hits + misses == 0 {
        0.0
    } else {
        (hits as f64 / (hits + misses) as f64) * 100.0
    };

    let memory_used_bytes = info.get_u64("used_memory").unwrap_or(0);
    let memory_peak_bytes = info.get_u64("used_memory_peak").unwrap_or(memory_used_bytes);
    let connected_clients = info.get_u64("connected_clients").unwrap_or(0);
    let evicted_keys = info.get_u64("evicted_keys").unwrap_or(0);
    let expired_keys = info.get_u64("expired_keys").unwrap_or(0);
    let role = info
        .get_str("role")
        .unwrap_or("unknown")
        .to_string();

    let total_keys = keyspace.iter().map(|d| d.keys).sum();

    let ops_per_sec = compute_ops_per_sec(info, rate_state, elapsed);

    let memory_used_ratio = if maxmemory_bytes > 0 {
        Some(memory_used_bytes as f64 / maxmemory_bytes as f64)
    } else {
        None
    };

    DerivedRedisMetrics {
        ops_per_sec,
        hit_rate_percent,
        memory_used_bytes,
        memory_peak_bytes,
        maxmemory_bytes,
        memory_used_ratio,
        connected_clients,
        evicted_keys,
        expired_keys,
        role,
        total_keys,
    }
}

fn compute_ops_per_sec(info: &RedisInfo, state: &mut RateState, elapsed: Duration) -> f64 {
    let instantaneous = info.get_u64("instantaneous_ops_per_sec");
    if let Some(ops) = instantaneous {
        state.last_instantaneous_ops = Some(ops);
        return ops as f64;
    }

    let total = info.get_u64("total_commands_processed").unwrap_or(0);
    let secs = elapsed.as_secs_f64().max(0.001);
    if let Some(prev) = state.last_total_commands {
        let delta = total.saturating_sub(prev);
        state.last_total_commands = Some(total);
        return delta as f64 / secs;
    }
    state.last_total_commands = Some(total);
    0.0
}

pub fn snapshot_payload(info: &RedisInfo, derived: DerivedRedisMetrics) -> (RedisInfoPayload, DerivedRedisMetricsPayload) {
    let info_payload = RedisInfoPayload {
        redis_version: info.get_str("redis_version").map(|s| s.to_string()),
        uptime_seconds: info.get_u64("uptime_in_seconds"),
        role: info.get_str("role").map(|s| s.to_string()),
    };
    let derived_payload = DerivedRedisMetricsPayload {
        ops_per_sec: derived.ops_per_sec,
        hit_rate_percent: derived.hit_rate_percent,
        memory_used_bytes: derived.memory_used_bytes,
        memory_peak_bytes: derived.memory_peak_bytes,
        maxmemory_bytes: derived.maxmemory_bytes,
        memory_used_ratio: derived.memory_used_ratio,
        connected_clients: derived.connected_clients,
        evicted_keys: derived.evicted_keys,
        expired_keys: derived.expired_keys,
        role: derived.role.clone(),
        total_keys: derived.total_keys,
    };
    (info_payload, derived_payload)
}

impl RedisSnapshot {
    pub fn from_parts(
        collected_at_ms: u64,
        info: RedisInfo,
        keyspace: Vec<RedisDbKeyspace>,
        slowlog: Vec<super::types::RedisSlowlogEntry>,
        derived: DerivedRedisMetrics,
    ) -> Self {
        let (info_payload, derived_payload) = snapshot_payload(&info, derived);
        Self {
            collected_at_ms,
            info: info_payload,
            keyspace,
            slowlog,
            derived: derived_payload,
            collector_state: None,
        }
    }
}

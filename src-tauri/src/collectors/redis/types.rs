use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisDbKeyspace {
    pub db: u32,
    pub keys: u64,
    pub expires: u64,
    pub avg_ttl_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisSlowlogEntry {
    pub id: u64,
    pub timestamp_unix: u64,
    pub duration_micros: u64,
    pub command: String,
    pub client: String,
}

#[derive(Debug, Clone, Default)]
pub struct RedisInfo {
    pub fields: std::collections::BTreeMap<String, String>,
}

impl RedisInfo {
    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.fields.get(key).and_then(|v| v.parse().ok())
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.fields.get(key).and_then(|v| v.parse().ok())
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(|s| s.as_str())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisSnapshot {
    pub collected_at_ms: u64,
    pub info: RedisInfoPayload,
    pub keyspace: Vec<RedisDbKeyspace>,
    pub slowlog: Vec<RedisSlowlogEntry>,
    pub derived: DerivedRedisMetricsPayload,
}

/// Flattened INFO fields needed by the UI (subset of full INFO).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisInfoPayload {
    pub redis_version: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedRedisMetricsPayload {
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

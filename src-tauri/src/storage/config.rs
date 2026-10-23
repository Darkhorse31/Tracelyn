use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RetentionConfig {
    pub raw_retention_days: u32,
    pub rollup_retention_days: u32,
    pub rollup_interval_secs: u64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            raw_retention_days: 7,
            rollup_retention_days: 90,
            rollup_interval_secs: 60,
        }
    }
}

impl RetentionConfig {
    pub fn raw_retention_ms(&self) -> i64 {
        i64::from(self.raw_retention_days) * 24 * 60 * 60 * 1000
    }

    pub fn rollup_retention_ms(&self) -> i64 {
        i64::from(self.rollup_retention_days) * 24 * 60 * 60 * 1000
    }
}

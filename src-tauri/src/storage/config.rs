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

    pub fn validate(&self) -> Result<(), String> {
        if !(1..=365).contains(&self.raw_retention_days) {
            return Err("Raw retention must be between 1 and 365 days".into());
        }
        if self.rollup_retention_days < self.raw_retention_days || self.rollup_retention_days > 3650 {
            return Err("Rollup retention must be at least the raw retention and at most 3650 days".into());
        }
        if !(10..=3600).contains(&self.rollup_interval_secs) {
            return Err("Rollup interval must be between 10 and 3600 seconds".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::RetentionConfig;

    #[test]
    fn default_is_valid() {
        assert!(RetentionConfig::default().validate().is_ok());
    }

    #[test]
    fn rejects_out_of_range_values() {
        let base = RetentionConfig::default();
        assert!(RetentionConfig { raw_retention_days: 0, ..base }.validate().is_err());
        assert!(RetentionConfig { rollup_retention_days: 3, ..base }.validate().is_err());
        assert!(RetentionConfig { rollup_interval_secs: 0, ..base }.validate().is_err());
    }
}

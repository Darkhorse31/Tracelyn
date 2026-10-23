use super::config::RetentionConfig;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
}

pub struct MetricStore {
    conn: Connection,
    retention: RetentionConfig,
}

#[derive(Debug, Clone)]
pub struct RawSample {
    pub server_id: String,
    pub metric_key: String,
    pub ts_ms: i64,
    pub value: f64,
}

impl MetricStore {
    pub fn open(path: PathBuf) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let mut store = Self {
            conn,
            retention: RetentionConfig::default(),
        };
        store.migrate()?;
        store.load_retention_config()?;
        store.bootstrap_alerts()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn,
            retention: RetentionConfig::default(),
        };
        store.migrate()?;
        store.bootstrap_alerts()?;
        Ok(store)
    }

    pub fn retention(&self) -> &RetentionConfig {
        &self.retention
    }

    pub fn set_retention(&mut self, config: RetentionConfig) -> Result<(), StoreError> {
        self.retention = config.clone();
        let json = serde_json::to_string(&config).map_err(|e| {
            StoreError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })?;
        self.conn.execute(
            "INSERT INTO app_settings (key, value) VALUES ('retention', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    fn migrate(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS metric_raw (
                server_id TEXT NOT NULL,
                metric_key TEXT NOT NULL,
                ts_ms INTEGER NOT NULL,
                value REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_metric_raw_lookup
                ON metric_raw (server_id, metric_key, ts_ms);
            CREATE TABLE IF NOT EXISTS metric_rollup (
                server_id TEXT NOT NULL,
                metric_key TEXT NOT NULL,
                bucket_ts_ms INTEGER NOT NULL,
                min_value REAL NOT NULL,
                max_value REAL NOT NULL,
                avg_value REAL NOT NULL,
                sample_count INTEGER NOT NULL,
                PRIMARY KEY (server_id, metric_key, bucket_ts_ms)
            );
            CREATE INDEX IF NOT EXISTS idx_metric_rollup_lookup
                ON metric_rollup (server_id, metric_key, bucket_ts_ms);
            ",
        )?;
        Ok(())
    }

    fn load_retention_config(&mut self) -> Result<(), StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM app_settings WHERE key = 'retention'")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let json: String = row.get(0)?;
            if let Ok(cfg) = serde_json::from_str::<RetentionConfig>(&json) {
                self.retention = cfg;
            }
        }
        Ok(())
    }

    pub fn insert_raw_batch(&self, samples: &[RawSample]) -> Result<(), StoreError> {
        if samples.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO metric_raw (server_id, metric_key, ts_ms, value)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for s in samples {
                stmt.execute(params![s.server_id, s.metric_key, s.ts_ms, s.value])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    fn bootstrap_alerts(&self) -> Result<(), StoreError> {
        use crate::alerts::{migrate_rules, seed_default_rules, AlertHistoryStore};

        migrate_rules(&self.conn).map_err(StoreError::from)?;
        AlertHistoryStore::migrate(&self.conn).map_err(StoreError::from)?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        seed_default_rules(&self.conn, now_ms).map_err(StoreError::from)?;
        Ok(())
    }
}

impl From<crate::alerts::RuleError> for StoreError {
    fn from(value: crate::alerts::RuleError) -> Self {
        match value {
            crate::alerts::RuleError::Sqlite(e) => StoreError::Sqlite(e),
        }
    }
}

impl From<crate::alerts::HistoryError> for StoreError {
    fn from(value: crate::alerts::HistoryError) -> Self {
        match value {
            crate::alerts::HistoryError::Sqlite(e) => StoreError::Sqlite(e),
        }
    }
}

pub fn default_db_path() -> PathBuf {
    let base = dirs::data_local_dir().expect("local data dir");
    base.join("Tracelyn").join("metrics.db")
}

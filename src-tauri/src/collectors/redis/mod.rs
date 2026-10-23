mod cli;
mod metrics;
mod parser;
mod types;

pub use cli::{build_redis_cli_invocation, RedisCliArgs};
pub use metrics::{derive_metrics, DerivedRedisMetrics, RateState};
pub use parser::{
    parse_config_get_maxmemory, parse_info_all, parse_info_keyspace, parse_slowlog_get,
};
pub use types::{RedisDbKeyspace, RedisSlowlogEntry, RedisSnapshot};

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::secrets::{get_secret, SecretError, SecretKind};

#[derive(Debug, Error)]
pub enum CollectError {
    #[error("remote command failed: {0}")]
    Remote(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("secret error: {0}")]
    Secret(#[from] SecretError),
}

/// Runs a shell command on the target host (SSH session, local test harness, etc.).
pub trait RemoteCommandRunner {
    fn run<'a>(
        &'a self,
        command: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;
}

pub struct RedisCollector<R: RemoteCommandRunner> {
    runner: R,
    server_id: String,
    redis_cli_path: String,
    rate_state: RateState,
    last_polled: Option<Instant>,
}

impl<R: RemoteCommandRunner> RedisCollector<R> {
    pub fn new(runner: R, server_id: impl Into<String>) -> Self {
        Self {
            runner,
            server_id: server_id.into(),
            redis_cli_path: "redis-cli".into(),
            rate_state: RateState::default(),
            last_polled: None,
        }
    }

    pub fn with_redis_cli_path(mut self, path: impl Into<String>) -> Self {
        self.redis_cli_path = path.into();
        self
    }

    fn redis_password(&self) -> Result<Option<String>, CollectError> {
        match get_secret(&self.server_id, SecretKind::RedisPassword) {
            Ok(value) => Ok(Some(value)),
            Err(SecretError::NotFound(_)) => Ok(None),
            Err(err) => Err(CollectError::Secret(err)),
        }
    }

    async fn exec_redis(&self, args: &RedisCliArgs) -> Result<String, CollectError> {
        let password = self.redis_password()?;
        let command = build_redis_cli_invocation(&self.redis_cli_path, args, password.as_deref());
        self.runner
            .run(&command)
            .await
            .map_err(CollectError::Remote)
    }

    pub async fn poll(&mut self) -> Result<RedisSnapshot, CollectError> {
        let now = Instant::now();
        let elapsed = self
            .last_polled
            .map(|t| now.duration_since(t))
            .unwrap_or(Duration::from_secs(5));
        self.last_polled = Some(now);

        let info_raw = self
            .exec_redis(&RedisCliArgs::new("INFO all"))
            .await?;
        let slowlog_raw = self
            .exec_redis(&RedisCliArgs::new("SLOWLOG GET 128"))
            .await?;
        let keyspace_raw = self
            .exec_redis(&RedisCliArgs::new("INFO keyspace"))
            .await?;
        let maxmemory_raw = self
            .exec_redis(&RedisCliArgs::new("CONFIG GET maxmemory"))
            .await?;

        let info = parse_info_all(&info_raw).map_err(CollectError::Parse)?;
        let slowlog = parse_slowlog_get(&slowlog_raw).map_err(CollectError::Parse)?;
        let keyspace = parse_info_keyspace(&keyspace_raw).map_err(CollectError::Parse)?;
        let maxmemory = parse_config_get_maxmemory(&maxmemory_raw).map_err(CollectError::Parse)?;

        let derived = derive_metrics(&info, &keyspace, maxmemory, &mut self.rate_state, elapsed);

        Ok(RedisSnapshot::from_parts(
            chrono_now_ms(),
            info,
            keyspace,
            slowlog,
            derived,
        ))
    }
}

fn chrono_now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct FixtureRunner {
        map: Arc<Mutex<HashMap<String, String>>>,
    }

    impl FixtureRunner {
        fn new() -> Self {
            Self {
                map: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        fn with(self, needle: &str, response: &str) -> Self {
            self.map
                .lock()
                .unwrap()
                .insert(needle.to_string(), response.to_string());
            self
        }
    }

    impl RemoteCommandRunner for FixtureRunner {
        fn run<'a>(
            &'a self,
            command: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
            let map = Arc::clone(&self.map);
            Box::pin(async move {
                let guard = map.lock().unwrap();
                for (key, value) in guard.iter() {
                    if command.contains(key) {
                        return Ok(value.clone());
                    }
                }
                Err(format!("no fixture for command: {command}"))
            })
        }
    }

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/redis/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        ))
        .expect("read fixture")
    }

    #[tokio::test]
    async fn collector_poll_parses_fixture_outputs() {
        let runner = FixtureRunner::new()
            .with("INFO all", &fixture("info_all.txt"))
            .with("SLOWLOG GET 128", &fixture("slowlog_get.txt"))
            .with("INFO keyspace", &fixture("info_keyspace.txt"))
            .with("CONFIG GET maxmemory", &fixture("config_maxmemory.txt"));

        let mut collector = RedisCollector::new(runner, "test-server");
        let snap = collector.poll().await.expect("poll");
        assert_eq!(snap.derived.role, "master");
        assert!(snap.derived.ops_per_sec > 0.0);
        assert!(snap.derived.hit_rate_percent > 90.0);
        assert_eq!(snap.keyspace.len(), 2);
        assert!(!snap.slowlog.is_empty());
    }
}

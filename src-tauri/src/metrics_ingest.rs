//! Generic hooks for collectors to push samples without depending on SSH internals.

use crate::storage::RawSample;
use std::future::Future;
use std::pin::Pin;

/// Runs a read-only remote command; wire to ConnectionManager when available.
pub type RemoteCommandRunner = Box<
    dyn Fn(&str) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> + Send + Sync,
>;

/// Reads incremental bytes from a remote stream (e.g. `tail -F`).
pub type RemoteStreamReader = Box<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send>> + Send + Sync,
>;

pub struct CollectorContext {
    pub run_command: RemoteCommandRunner,
    pub read_stream: Option<RemoteStreamReader>,
}

pub trait MetricCollector {
    fn collect(&self, ctx: &CollectorContext) -> Pin<Box<dyn Future<Output = Vec<RawSample>> + Send>>;
}

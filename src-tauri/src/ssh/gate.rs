//! Shared retry, error class, and channel-cap helpers for one SSH connection.
//!
//! OpenSSH `MaxSessions` defaults to 10. Collectors, log follows, and infra polls
//! open exec channels on the same connection, so the cap stays under that.

use std::future::Future;
use std::time::Duration;

#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use tokio::sync::Semaphore;

pub const MAX_CHANNELS: usize = 6;
/// Long-lived follows stop before this many permits are left, so short execs can still run.
pub const RESERVED_EXEC_SLOTS: usize = 2;
pub const MAX_ATTEMPTS: u32 = 3;
pub const BACKOFF_BASE_MS: u64 = 250;
pub const BACKOFF_CAP_MS: u64 = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Transient,
    Persistent,
}

/// Why a single attempt failed. Callers map transport errors onto this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailKind {
    /// Session or channel is dead. Rebuild the session before the next try.
    Stale,
    /// Remote channel limit or a full client window. Wait and try again.
    Busy,
    /// The command or the socket timed out. The session may still be alive.
    Timeout,
    Persistent,
}

pub fn error_class(kind: FailKind) -> ErrorClass {
    match kind {
        FailKind::Persistent => ErrorClass::Persistent,
        FailKind::Stale | FailKind::Busy | FailKind::Timeout => ErrorClass::Transient,
    }
}

pub fn needs_reconnect(kind: FailKind) -> bool {
    kind == FailKind::Stale
}

/// Exponential delay. `jitter_unit` subtracts up to 20% so retries do not line up.
pub fn backoff_ms(attempt: u32, jitter_unit: u64) -> u64 {
    let exp = BACKOFF_BASE_MS.saturating_mul(1u64 << attempt.min(4));
    let capped = exp.min(BACKOFF_CAP_MS);
    let span = (capped / 5).max(1);
    capped.saturating_sub(jitter_unit % span)
}

pub async fn retry_loop<T, E, Run, RF, Rec, NF, J, Sleep, SF>(
    mut run: Run,
    mut reconnect: Rec,
    mut kind_of: impl FnMut(&E) -> FailKind,
    mut jitter: J,
    mut sleep: Sleep,
) -> Result<T, E>
where
    Run: FnMut() -> RF,
    RF: Future<Output = Result<T, E>>,
    Rec: FnMut() -> NF,
    NF: Future<Output = Result<(), E>>,
    J: FnMut(u32) -> u64,
    Sleep: FnMut(Duration) -> SF,
    SF: Future<Output = ()>,
{
    let mut attempt = 0u32;
    loop {
        match run().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                let kind = kind_of(&err);
                if error_class(kind) == ErrorClass::Persistent || attempt + 1 >= MAX_ATTEMPTS {
                    return Err(err);
                }
                if needs_reconnect(kind) {
                    if let Err(re) = reconnect().await {
                        let rk = kind_of(&re);
                        if error_class(rk) == ErrorClass::Persistent || attempt + 1 >= MAX_ATTEMPTS
                        {
                            return Err(re);
                        }
                    }
                }
                let delay = Duration::from_millis(backoff_ms(attempt, jitter(attempt)));
                sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

/// Mockable command runner. Production SSH calls [`retry_loop`] directly so the
/// original error text is kept. Tests use this trait.
#[cfg(test)]
#[async_trait]
pub trait CommandExecutor: Send + Sync {
    async fn execute(&self, command: &str) -> Result<String, FailKind>;
    async fn reconnect(&self) -> Result<(), FailKind>;
}

#[cfg(test)]
pub async fn run_executor(exec: &impl CommandExecutor, command: &str) -> Result<String, FailKind> {
    retry_loop(
        || exec.execute(command),
        || exec.reconnect(),
        |kind| *kind,
        |_| 0,
        |_| async {},
    )
    .await
}

#[cfg(test)]
pub fn channel_slots() -> Arc<Semaphore> {
    Arc::new(Semaphore::new(MAX_CHANNELS))
}

/// `available` is the permit count before a follow takes one.
pub fn follow_may_open(available: usize) -> bool {
    available > RESERVED_EXEC_SLOTS
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn classifies_stale_busy_and_auth() {
        assert_eq!(error_class(FailKind::Stale), ErrorClass::Transient);
        assert!(needs_reconnect(FailKind::Stale));
        assert_eq!(error_class(FailKind::Busy), ErrorClass::Transient);
        assert!(!needs_reconnect(FailKind::Busy));
        assert_eq!(error_class(FailKind::Timeout), ErrorClass::Transient);
        assert!(!needs_reconnect(FailKind::Timeout));
        assert_eq!(error_class(FailKind::Persistent), ErrorClass::Persistent);
        assert!(!needs_reconnect(FailKind::Persistent));
    }

    #[test]
    fn backoff_doubles_from_250ms_and_caps_at_4s() {
        assert_eq!(backoff_ms(0, 0), 250);
        assert_eq!(backoff_ms(1, 0), 500);
        assert_eq!(backoff_ms(2, 0), 1_000);
        assert_eq!(backoff_ms(3, 0), 2_000);
        assert_eq!(backoff_ms(4, 0), 4_000);
        assert_eq!(backoff_ms(8, 0), 4_000);
        let jittered = backoff_ms(2, 1);
        assert!(jittered < 1_000);
        assert!(jittered >= 800);
    }

    #[tokio::test]
    async fn semaphore_never_exceeds_six_channels() {
        let slots = channel_slots();
        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..20 {
            let slots = slots.clone();
            let current = current.clone();
            let peak = peak.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = slots.acquire().await.expect("semaphore open");
                let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                current.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for task in tasks {
            task.await.expect("task");
        }
        assert_eq!(peak.load(Ordering::SeqCst), MAX_CHANNELS);
    }

    #[test]
    fn follows_leave_two_slots_for_exec() {
        assert!(follow_may_open(MAX_CHANNELS));
        assert!(follow_may_open(RESERVED_EXEC_SLOTS + 1));
        assert!(!follow_may_open(RESERVED_EXEC_SLOTS));
        assert!(!follow_may_open(0));
    }

    struct Flaky {
        runs: AtomicUsize,
        reconnects: AtomicUsize,
    }

    #[async_trait]
    impl CommandExecutor for Flaky {
        async fn execute(&self, _command: &str) -> Result<String, FailKind> {
            let n = self.runs.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Err(FailKind::Stale)
            } else {
                Ok("ok".into())
            }
        }

        async fn reconnect(&self) -> Result<(), FailKind> {
            self.reconnects.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn executor_reconnects_a_stale_channel_then_succeeds() {
        let exec = Flaky {
            runs: AtomicUsize::new(0),
            reconnects: AtomicUsize::new(0),
        };
        let out = run_executor(&exec, "cat /proc/stat").await.expect("recovered");
        assert_eq!(out, "ok");
        assert_eq!(exec.reconnects.load(Ordering::SeqCst), 1);
        assert_eq!(exec.runs.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn persistent_failure_does_not_reconnect() {
        struct Auth;

        #[async_trait]
        impl CommandExecutor for Auth {
            async fn execute(&self, _command: &str) -> Result<String, FailKind> {
                Err(FailKind::Persistent)
            }
            async fn reconnect(&self) -> Result<(), FailKind> {
                panic!("reconnect on a persistent error");
            }
        }

        let err = run_executor(&Auth, "true").await.expect_err("auth");
        assert_eq!(err, FailKind::Persistent);
    }

    #[tokio::test]
    async fn stale_channel_stops_after_three_attempts() {
        let runs = AtomicUsize::new(0);
        let reconnects = AtomicUsize::new(0);
        let err = retry_loop(
            || {
                runs.fetch_add(1, Ordering::SeqCst);
                async { Err::<(), FailKind>(FailKind::Stale) }
            },
            || {
                reconnects.fetch_add(1, Ordering::SeqCst);
                async { Ok(()) }
            },
            |kind| *kind,
            |_| 0,
            |_| async {},
        )
        .await
        .expect_err("still stale");
        assert_eq!(err, FailKind::Stale);
        assert_eq!(runs.load(Ordering::SeqCst), 3);
        assert_eq!(reconnects.load(Ordering::SeqCst), 2);
    }
}

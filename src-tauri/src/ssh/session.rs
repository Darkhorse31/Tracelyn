use super::gate::{self, FailKind, MAX_CHANNELS};
use crate::config::{AuthType, ServerRecord};
use crate::secrets::{get_secret, SecretError, SecretKind};
use crate::ssh::host_keys::{HostKeyPrompt, HostKeyStore, KeyCheck};
use async_trait::async_trait;
use russh::client;
use russh::ChannelMsg;
use russh_keys::load_secret_key;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{Mutex, OwnedSemaphorePermit};

#[derive(Debug, Error)]
pub enum SshError {
    #[error("SSH: {0}")]
    Russh(#[from] russh::Error),
    #[error("keys: {0}")]
    Keys(#[from] russh_keys::Error),
    #[error("secret: {0}")]
    Secret(#[from] SecretError),
    #[error("host key: {0}")]
    HostKey(#[from] crate::ssh::host_keys::HostKeyError),
    #[error("HOSTKEY:{0}")]
    NeedsTrust(HostKeyPrompt),
    #[error("authentication failed for {0}")]
    AuthFailed(String),
    #[error("command failed: {0}")]
    Command(String),
    #[error("{0}")]
    Message(String),
    /// The russh session task has exited, or the handle was already closed.
    #[error("SSH session is stale")]
    StaleSession,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub success: bool,
    pub os_info: String,
    pub hostname: Option<String>,
}

pub struct SshHandler {
    host: String,
    port: u16,
    host_keys: Arc<Mutex<HostKeyStore>>,
    prompt: Arc<Mutex<Option<HostKeyPrompt>>>,
}

type Handle = client::Handle<SshHandler>;

/// One SSH connection shared by collectors. Channels take a permit from `slots`
/// and the handle can be replaced after the session task dies.
pub struct SshSession {
    inner: Mutex<Handle>,
    slots: Arc<tokio::sync::Semaphore>,
    server: ServerRecord,
    host_keys: Arc<Mutex<HostKeyStore>>,
    pending_prompt: Mutex<Option<HostKeyPrompt>>,
    reconnecting: Mutex<()>,
}

/// An open SSH channel that holds its concurrency permit until it is dropped.
pub struct SshChannel {
    channel: Option<russh::Channel<client::Msg>>,
    _permit: OwnedSemaphorePermit,
}

impl Drop for SshChannel {
    fn drop(&mut self) {
        let Some(channel) = self.channel.take() else {
            return;
        };
        // ponytail: close on a spawned task. Callers also close after a full read.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = channel.close().await;
            });
        }
    }
}

impl Deref for SshChannel {
    type Target = russh::Channel<client::Msg>;

    fn deref(&self) -> &Self::Target {
        self.channel.as_ref().expect("ssh channel still open")
    }
}

impl DerefMut for SshChannel {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.channel.as_mut().expect("ssh channel still open")
    }
}

impl SshSession {
    /// Upload over the existing authenticated connection, without a shell or TCP listener.
    pub async fn upload_companion_file(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;
        let mut channel = self.channel_open_session().await.map_err(|e| e.to_string())?;
        channel.request_subsystem(true, "sftp").await.map_err(|e| e.to_string())?;
        let stream = channel.channel.take().ok_or("SFTP channel unavailable")?.into_stream();
        let sftp = russh_sftp::client::SftpSession::new(stream).await.map_err(|e| e.to_string())?;
        let mut file = sftp.create(path).await.map_err(|e| e.to_string())?;
        file.write_all(bytes).await.map_err(|e| e.to_string())?;
        file.shutdown().await.map_err(|e| e.to_string())?;
        sftp.close().await.map_err(|e| e.to_string())
    }
    pub async fn take_host_key_prompt(&self) -> Option<HostKeyPrompt> {
        self.pending_prompt.lock().await.take()
    }

    pub async fn is_closed(&self) -> bool {
        self.inner.lock().await.is_closed()
    }

    pub async fn disconnect(
        &self,
        reason: russh::Disconnect,
        description: &str,
        language_tag: &str,
    ) -> Result<(), SshError> {
        self.inner
            .lock()
            .await
            .disconnect(reason, description, language_tag)
            .await
            .map_err(SshError::from)
    }

    /// Rebuild a dead session. Host key checks are the same path as the first connect.
    pub async fn reconnect(&self) -> Result<(), SshError> {
        let _gate = self.reconnecting.lock().await;
        if !self.inner.lock().await.is_closed() {
            return Ok(());
        }
        let fresh = match connect_handle(&self.server, None, None, self.host_keys.clone()).await {
            Ok(handle) => handle,
            Err(SshError::NeedsTrust(prompt)) => {
                *self.pending_prompt.lock().await = Some(prompt.clone());
                return Err(SshError::NeedsTrust(prompt));
            }
            Err(err) => return Err(err),
        };
        let old = {
            let mut guard = self.inner.lock().await;
            if !guard.is_closed() {
                drop(fresh);
                return Ok(());
            }
            std::mem::replace(&mut *guard, fresh)
        };
        let _ = old
            .disconnect(russh::Disconnect::ByApplication, "", "English")
            .await;
        Ok(())
    }

    pub async fn channel_open_session(&self) -> Result<SshChannel, SshError> {
        gate::retry_loop(
            || self.open_exec_once(),
            || self.reconnect(),
            fail_kind,
            jitter_unit,
            |delay| async move { tokio::time::sleep(delay).await },
        )
        .await
    }

    pub async fn channel_open_direct_tcpip(
        &self,
        host: &str,
        port: u32,
        originator: &str,
        originator_port: u32,
    ) -> Result<SshChannel, SshError> {
        let host = host.to_string();
        let originator = originator.to_string();
        gate::retry_loop(
            || {
                let host = host.clone();
                let originator = originator.clone();
                async move {
                    self.open_direct_once(&host, port, &originator, originator_port)
                        .await
                }
            },
            || self.reconnect(),
            fail_kind,
            jitter_unit,
            |delay| async move { tokio::time::sleep(delay).await },
        )
        .await
    }

    async fn acquire(&self) -> Result<OwnedSemaphorePermit, SshError> {
        self.slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| SshError::Message("collector stopped".into()))
    }

    async fn open_exec_once(&self) -> Result<SshChannel, SshError> {
        self.open_channel(true).await
    }

    async fn open_follow_once(&self) -> Result<SshChannel, SshError> {
        self.open_channel(false).await
    }

    /// Long-lived exec. Fails without waiting when the channel cap would be exceeded.
    pub async fn open_follow_channel(&self) -> Result<SshChannel, SshError> {
        self.open_follow_once().await
    }

    async fn open_channel(&self, wait_for_slot: bool) -> Result<SshChannel, SshError> {
        let permit = if wait_for_slot {
            self.acquire().await?
        } else if !gate::follow_may_open(self.slots.available_permits()) {
            return Err(SshError::Message(
                "too many SSH channels are already open".into(),
            ));
        } else {
            let permit = self.slots.clone().try_acquire_owned().map_err(|_| {
                SshError::Message("too many SSH channels are already open".into())
            })?;
            if self.slots.available_permits() < gate::RESERVED_EXEC_SLOTS {
                drop(permit);
                return Err(SshError::Message(
                    "too many SSH channels are already open".into(),
                ));
            }
            permit
        };
        let handle = self.inner.lock().await;
        if handle.is_closed() {
            return Err(SshError::StaleSession);
        }
        let channel = handle.channel_open_session().await.map_err(lift_russh)?;
        Ok(SshChannel {
            channel: Some(channel),
            _permit: permit,
        })
    }

    async fn open_direct_once(
        &self,
        host: &str,
        port: u32,
        originator: &str,
        originator_port: u32,
    ) -> Result<SshChannel, SshError> {
        let permit = self.acquire().await?;
        let handle = self.inner.lock().await;
        if handle.is_closed() {
            return Err(SshError::StaleSession);
        }
        let channel = handle
            .channel_open_direct_tcpip(host, port, originator, originator_port)
            .await
            .map_err(lift_russh)?;
        Ok(SshChannel {
            channel: Some(channel),
            _permit: permit,
        })
    }
}

#[async_trait]
impl client::Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        let store = self.host_keys.lock().await;
        match store.inspect(&self.host, self.port, server_public_key) {
            KeyCheck::Trusted => Ok(true),
            KeyCheck::Unknown { fingerprint } => {
                *self.prompt.lock().await = Some(HostKeyPrompt {
                    host: self.host.clone(),
                    port: self.port,
                    fingerprint,
                    changed: false,
                    expected: None,
                });
                Ok(false)
            }
            KeyCheck::Changed { expected, actual } => {
                *self.prompt.lock().await = Some(HostKeyPrompt {
                    host: self.host.clone(),
                    port: self.port,
                    fingerprint: actual,
                    changed: true,
                    expected: Some(expected),
                });
                Ok(false)
            }
        }
    }
}

pub async fn test_connection(
    server: &ServerRecord,
    password_override: Option<String>,
    key_passphrase_override: Option<String>,
    host_keys: Arc<Mutex<HostKeyStore>>,
) -> Result<ConnectionTestResult, SshError> {
    let session = connect(server, password_override, key_passphrase_override, host_keys).await?;
    let os_info = run_command(&session, "cat /etc/os-release 2>/dev/null || uname -a")
        .await?
        .trim()
        .to_string();
    let hostname = run_command(&session, "hostname -f 2>/dev/null || hostname")
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let _ = session
        .disconnect(russh::Disconnect::ByApplication, "", "English")
        .await;
    Ok(ConnectionTestResult {
        success: true,
        os_info,
        hostname,
    })
}

pub async fn connect_persistent(
    server: &ServerRecord,
    host_keys: Arc<Mutex<HostKeyStore>>,
) -> Result<SshSession, SshError> {
    connect(server, None, None, host_keys).await
}

fn wrap_handle(
    handle: Handle,
    server: &ServerRecord,
    host_keys: Arc<Mutex<HostKeyStore>>,
) -> SshSession {
    SshSession {
        inner: Mutex::new(handle),
        slots: Arc::new(tokio::sync::Semaphore::new(MAX_CHANNELS)),
        server: server.clone(),
        host_keys,
        pending_prompt: Mutex::new(None),
        reconnecting: Mutex::new(()),
    }
}

async fn connect(
    server: &ServerRecord,
    password_override: Option<String>,
    key_passphrase_override: Option<String>,
    host_keys: Arc<Mutex<HostKeyStore>>,
) -> Result<SshSession, SshError> {
    let handle = connect_handle(
        server,
        password_override,
        key_passphrase_override,
        host_keys.clone(),
    )
    .await?;
    Ok(wrap_handle(handle, server, host_keys))
}

async fn connect_handle(
    server: &ServerRecord,
    password_override: Option<String>,
    key_passphrase_override: Option<String>,
    host_keys: Arc<Mutex<HostKeyStore>>,
) -> Result<Handle, SshError> {
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    });
    let prompt = Arc::new(Mutex::new(None));
    let handler = SshHandler {
        host: server.host.clone(),
        port: server.port,
        host_keys,
        prompt: prompt.clone(),
    };
    let addr = (server.host.as_str(), server.port);
    let mut session = match client::connect(config, addr, handler).await {
        Ok(session) => session,
        Err(err) => {
            if let Some(prompt) = prompt.lock().await.clone() {
                return Err(SshError::NeedsTrust(prompt));
            }
            return Err(SshError::Russh(err));
        }
    };

    let authed = match server.auth_type {
        AuthType::Password => {
            let password = match password_override {
                Some(p) => p,
                None => get_secret(&server.id, SecretKind::Password)?,
            };
            session.authenticate_password(&server.username, &password).await?
        }
        AuthType::Key => {
            let key_path = server
                .key_path
                .as_ref()
                .ok_or_else(|| SshError::Message("SSH key path is required".into()))?;
            let passphrase = if let Some(p) = key_passphrase_override.filter(|p| !p.is_empty()) {
                Some(p)
            } else {
                match get_secret(&server.id, SecretKind::KeyPassphrase) {
                    Ok(p) => Some(p),
                    Err(SecretError::NotFound(_)) => None,
                    Err(e) => return Err(SshError::Secret(e)),
                }
            };
            let passphrase_ref = passphrase.as_deref();
            let key_pair = load_secret_key(key_path, passphrase_ref)?;
            session
                .authenticate_publickey(&server.username, Arc::new(key_pair))
                .await?
        }
    };

    if !authed {
        return Err(SshError::AuthFailed(server.username.clone()));
    }

    Ok(session)
}

/// Stream access-log lines from `tail -n 500 -F` on a dedicated SSH channel.
pub async fn start_tail_follow(
    session: &SshSession,
    log_path: &str,
) -> Result<tokio::sync::mpsc::Receiver<Result<String, String>>, SshError> {
    let escaped = log_path.replace('\'', "'\\''");
    let cmd = format!("tail -n 500 -F -- '{escaped}' 2>&1");
    start_command_follow(session, &cmd).await
}

/// Follow a remote command (tail, journalctl, docker logs). Bounded channel drops
/// lines when the collector falls behind. The channel is closed when the command ends.
pub async fn start_command_follow(
    session: &SshSession,
    cmd: &str,
) -> Result<tokio::sync::mpsc::Receiver<Result<String, String>>, SshError> {
    let cmd = cmd.to_string();
    let channel = gate::retry_loop(
        || {
            let cmd = cmd.clone();
            async move {
                let channel = session.open_follow_once().await?;
                if let Err(err) = channel.exec(false, cmd).await {
                    let _ = channel.close().await;
                    return Err(lift_russh(err));
                }
                Ok(channel)
            }
        },
        || session.reconnect(),
        fail_kind,
        jitter_unit,
        |delay| async move { tokio::time::sleep(delay).await },
    )
    .await?;
    let (tx, rx) = tokio::sync::mpsc::channel(2048);
    tauri::async_runtime::spawn(async move {
        let mut channel = channel;
        let mut buf = String::new();
        let mut saw_exit = false;
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    buf.push_str(&String::from_utf8_lossy(&data));
                    while let Some(pos) = buf.find('\n') {
                        let line = buf[..pos].trim_end_matches('\r').to_string();
                        buf = buf[pos + 1..].to_string();
                        if line.is_empty() {
                            continue;
                        }
                        let item = if line.contains("Permission denied")
                            || line.contains("cannot open")
                            || line.contains("No such file")
                        {
                            Err(line)
                        } else {
                            Ok(line)
                        };
                        match tx.try_send(item) {
                            Ok(()) => {}
                            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {}
                            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                                let _ = channel.close().await;
                                return;
                            }
                        }
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    saw_exit = true;
                    if exit_status != 0 {
                        let _ = tx
                            .try_send(Err(format!("follow exited with status {exit_status}")))
                            .ok();
                    }
                }
                Some(ChannelMsg::Eof) => break,
                Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
            let _ = saw_exit;
        }
        let _ = channel.close().await;
    });
    Ok(rx)
}

pub async fn run_command_bytes(session: &SshSession, command: &str) -> Result<Vec<u8>, SshError> {
    let command = command.to_string();
    gate::retry_loop(
        || {
            let command = command.clone();
            async move {
                let collected = exec_once(session, &command, false).await?;
                finish_bytes(&command, collected)
            }
        },
        || session.reconnect(),
        fail_kind,
        jitter_unit,
        |delay| async move { tokio::time::sleep(delay).await },
    )
    .await
}

/// Like [`run_command`], but stderr is kept and the command text is not returned.
/// Redis uses this so a refused connection is visible and a password in the command is not.
pub async fn run_command_merged(session: &SshSession, command: &str) -> Result<String, SshError> {
    let command = command.to_string();
    gate::retry_loop(
        || {
            let command = command.clone();
            async move {
                let joined = tokio::time::timeout(Duration::from_secs(8), exec_once(session, &command, true)).await;
                match joined {
                    Ok(result) => finish_merged(result?),
                    Err(_) => Err(SshError::Command("timed out".into())),
                }
            }
        },
        || session.reconnect(),
        fail_kind,
        jitter_unit,
        |delay| async move { tokio::time::sleep(delay).await },
    )
    .await
}

pub async fn run_command(session: &SshSession, command: &str) -> Result<String, SshError> {
    let command = command.to_string();
    gate::retry_loop(
        || {
            let command = command.clone();
            async move {
                let collected = exec_once(session, &command, false).await?;
                finish_stdout(&command, collected)
            }
        },
        || session.reconnect(),
        fail_kind,
        jitter_unit,
        |delay| async move { tokio::time::sleep(delay).await },
    )
    .await
}

struct Collected {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit: Option<u32>,
}

/// Open a fresh exec channel, read it to completion, then close it. The channel is never reused.
async fn exec_once(session: &SshSession, command: &str, keep_stderr: bool) -> Result<Collected, SshError> {
    let mut channel = session.open_exec_once().await?;
    if let Err(err) = channel.exec(true, command.to_string()).await {
        let _ = channel.close().await;
        return Err(lift_russh(err));
    }
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut exit = None;
    let mut eof = false;
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Data { data }) => stdout.extend_from_slice(&data),
            Some(ChannelMsg::ExtendedData { data, .. }) if keep_stderr => {
                stderr.extend_from_slice(&data);
            }
            Some(ChannelMsg::ExitStatus { exit_status }) => exit = Some(exit_status),
            Some(ChannelMsg::Eof) => eof = true,
            Some(ChannelMsg::Close) | None => break,
            _ => {}
        }
        if exit.is_some() && eof {
            break;
        }
    }
    let _ = channel.close().await;
    Ok(Collected { stdout, stderr, exit })
}

fn finish_stdout(command: &str, collected: Collected) -> Result<String, SshError> {
    let output = String::from_utf8_lossy(&collected.stdout).into_owned();
    if collected.exit.unwrap_or(0) != 0 && output.is_empty() {
        return Err(SshError::Command(format!(
            "exit status {} for `{command}`",
            collected.exit.unwrap_or(0)
        )));
    }
    Ok(output)
}

fn finish_bytes(command: &str, collected: Collected) -> Result<Vec<u8>, SshError> {
    if collected.exit.unwrap_or(0) != 0 && collected.stdout.is_empty() {
        return Err(SshError::Command(format!(
            "exit status {} for `{command}`",
            collected.exit.unwrap_or(0)
        )));
    }
    Ok(collected.stdout)
}

fn finish_merged(collected: Collected) -> Result<String, SshError> {
    let mut output = String::from_utf8_lossy(&collected.stdout).into_owned();
    output.push_str(&String::from_utf8_lossy(&collected.stderr));
    if let Some(code) = collected.exit {
        if code != 0 {
            if output.trim().is_empty() {
                output = format!("exit status {code}");
            }
            return Err(SshError::Command(output));
        }
    }
    Ok(output)
}

fn fail_kind(err: &SshError) -> FailKind {
    match err {
        SshError::StaleSession => FailKind::Stale,
        SshError::NeedsTrust(_)
        | SshError::AuthFailed(_)
        | SshError::HostKey(_)
        | SshError::Secret(_)
        | SshError::Keys(_) => FailKind::Persistent,
        SshError::Command(msg) | SshError::Message(msg) => {
            if msg.contains("timed out") {
                FailKind::Timeout
            } else {
                FailKind::Persistent
            }
        }
        SshError::Russh(inner) => match inner {
            russh::Error::SendError
            | russh::Error::Disconnect
            | russh::Error::HUP
            | russh::Error::KeepaliveTimeout
            | russh::Error::ConnectionTimeout
            | russh::Error::IO(_) => FailKind::Stale,
            russh::Error::ChannelOpenFailure(_) => FailKind::Busy,
            russh::Error::Elapsed(_) => FailKind::Timeout,
            _ => FailKind::Persistent,
        },
    }
}

fn lift_russh(err: russh::Error) -> SshError {
    match &err {
        russh::Error::SendError
        | russh::Error::Disconnect
        | russh::Error::HUP
        | russh::Error::KeepaliveTimeout
        | russh::Error::ConnectionTimeout => SshError::StaleSession,
        _ => SshError::Russh(err),
    }
}

fn jitter_unit(attempt: u32) -> u64 {
    let tick = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    tick.wrapping_add(u64::from(attempt).wrapping_mul(17))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_error_and_stale_session_reconnect() {
        assert_eq!(fail_kind(&SshError::StaleSession), FailKind::Stale);
        assert_eq!(
            fail_kind(&SshError::Russh(russh::Error::SendError)),
            FailKind::Stale
        );
        assert!(gate::needs_reconnect(FailKind::Stale));
    }

    #[test]
    fn auth_and_remote_command_failures_are_persistent() {
        assert_eq!(
            fail_kind(&SshError::AuthFailed("root".into())),
            FailKind::Persistent
        );
        assert_eq!(
            fail_kind(&SshError::Command("exit status 1 for `true`".into())),
            FailKind::Persistent
        );
        assert!(!gate::needs_reconnect(FailKind::Persistent));
    }

    #[test]
    fn channel_limit_waits_without_reconnecting() {
        let err = SshError::Russh(russh::Error::ChannelOpenFailure(
            russh::ChannelOpenFailure::ResourceShortage,
        ));
        assert_eq!(fail_kind(&err), FailKind::Busy);
        assert_eq!(gate::error_class(FailKind::Busy), gate::ErrorClass::Transient);
        assert!(!gate::needs_reconnect(FailKind::Busy));
    }

    #[test]
    fn timeouts_retry_without_marking_the_session_dead() {
        assert_eq!(
            fail_kind(&SshError::Command("timed out".into())),
            FailKind::Timeout
        );
        assert!(!gate::needs_reconnect(FailKind::Timeout));
    }
}

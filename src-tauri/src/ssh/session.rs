use crate::config::{AuthType, ServerRecord};
use crate::secrets::{get_secret, SecretError, SecretKind};
use crate::ssh::host_keys::HostKeyStore;
use async_trait::async_trait;
use russh::client;
use russh::ChannelMsg;
use russh_keys::load_secret_key;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;

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
    #[error("authentication failed for {0}")]
    AuthFailed(String),
    #[error("command failed: {0}")]
    Command(String),
    #[error("{0}")]
    Message(String),
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
}

pub type SshSession = client::Handle<SshHandler>;

#[async_trait]
impl client::Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        let mut store = self.host_keys.lock().await;
        match store.verify_or_trust(&self.host, self.port, server_public_key) {
            Ok(()) => Ok(true),
            Err(crate::ssh::host_keys::HostKeyError::Changed { .. }) => Ok(false),
            Err(_) => Ok(false),
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

pub async fn connect_and_probe(
    server: &ServerRecord,
    host_keys: Arc<Mutex<HostKeyStore>>,
) -> Result<ConnectionTestResult, SshError> {
    test_connection(server, None, None, host_keys).await
}

pub async fn connect_persistent(
    server: &ServerRecord,
    host_keys: Arc<Mutex<HostKeyStore>>,
) -> Result<SshSession, SshError> {
    connect(server, None, None, host_keys).await
}

async fn connect(
    server: &ServerRecord,
    password_override: Option<String>,
    key_passphrase_override: Option<String>,
    host_keys: Arc<Mutex<HostKeyStore>>,
) -> Result<SshSession, SshError> {
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    });
    let handler = SshHandler {
        host: server.host.clone(),
        port: server.port,
        host_keys,
    };
    let addr = (server.host.as_str(), server.port);
    let mut session = client::connect(config, addr, handler).await?;

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

/// Stream access-log lines from `tail -F` on a dedicated SSH channel.
pub async fn start_tail_follow(
    session: &SshSession,
    log_path: &str,
) -> Result<tokio::sync::mpsc::UnboundedReceiver<Result<String, String>>, SshError> {
    let mut channel = session.channel_open_session().await?;
    let escaped = log_path.replace('\'', "'\\''");
    let cmd = format!("tail -n 0 -F -- '{}' 2>&1", escaped);
    channel.exec(false, cmd).await?;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tauri::async_runtime::spawn(async move {
        let mut buf = String::new();
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => {
                    buf.push_str(&String::from_utf8_lossy(&data));
                    while let Some(pos) = buf.find('\n') {
                        let line = buf[..pos].trim_end_matches('\r').to_string();
                        buf = buf[pos + 1..].to_string();
                        if line.is_empty() {
                            continue;
                        }
                        if line.contains("Permission denied")
                            || line.contains("cannot open")
                            || line.contains("No such file")
                        {
                            let _ = tx.send(Err(line));
                            continue;
                        }
                        let _ = tx.send(Ok(line));
                    }
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    if exit_status != 0 {
                        let _ = tx.send(Err(format!("tail exited with status {}", exit_status)));
                    }
                    break;
                }
                ChannelMsg::Eof => break,
                _ => {}
            }
        }
    });
    Ok(rx)
}

pub async fn run_command(session: &SshSession, command: &str) -> Result<String, SshError> {
    let mut channel = session.channel_open_session().await?;
    channel.exec(true, command).await?;
    let mut output = String::new();
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Data { data }) => {
                output.push_str(&String::from_utf8_lossy(&data));
            }
            Some(ChannelMsg::ExitStatus { exit_status }) => {
                if exit_status != 0 && output.is_empty() {
                    return Err(SshError::Command(format!(
                        "exit status {} for `{}`",
                        exit_status,
                        command
                    )));
                }
                break;
            }
            Some(ChannelMsg::Eof) | None => break,
            _ => {}
        }
    }
    Ok(output)
}

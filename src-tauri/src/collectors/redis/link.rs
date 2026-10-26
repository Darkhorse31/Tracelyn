//! Reach Redis on the server. Prefer an SSH direct-tcpip channel to 127.0.0.1.
//! If that fails, run `redis-cli -h 127.0.0.1` on the exec channel with stdin closed.

use std::time::Duration;

use russh::ChannelMsg;

use crate::ssh::{run_command_merged, SshSession};

use super::cli::{build_redis_cli_invocation, RedisCliArgs};
use super::parser::{parse_config_get_maxmemory, parse_slowlog_get};
use super::resp::{
    bulk_text, encode_command, handshake_commands, maxmemory_from_resp, slowlog_from_resp, Resp,
    RespParser,
};
use super::types::RedisSlowlogEntry;

#[derive(Debug, Clone)]
pub struct Dial {
    pub host: String,
    pub port: u16,
    pub socket: Option<String>,
    pub docker: Option<String>,
    pub tls: bool,
    pub bind_label: String,
}

impl Dial {
    pub fn loopback(port: u16) -> Self {
        Self {
            host: "127.0.0.1".into(),
            port,
            socket: None,
            docker: None,
            tls: false,
            bind_label: format!("127.0.0.1:{port}"),
        }
    }
}

#[derive(Debug)]
pub struct Fetched {
    pub info: String,
    pub keyspace: String,
    pub slowlog: Vec<RedisSlowlogEntry>,
    pub maxmemory: u64,
    /// Which path produced the sample (`direct-tcpip` or `redis-cli`).
    #[allow(dead_code)]
    pub method: String,
}

#[derive(Debug, Clone)]
pub enum RedisAccessError {
    NeedsPassword,
    ProtectedMode { host: String, port: u16 },
    Refused { host: String, port: u16, bind: String, method: String },
    TimedOut { host: String, port: u16, method: String },
    Other(String),
}

impl RedisAccessError {
    pub fn phase(&self) -> &'static str {
        match self {
            Self::NeedsPassword => "auth_required",
            Self::ProtectedMode { .. } => "protected_mode",
            Self::Refused { .. } => "connection_refused",
            Self::TimedOut { .. } => "timed_out",
            Self::Other(_) => "error",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::NeedsPassword => "Redis needs a password.".into(),
            Self::ProtectedMode { host, port } => format!(
                "Redis protected mode rejected the connection to {host}:{port}. It only accepts loopback clients until a password is set."
            ),
            Self::Refused { host, port, bind, method } => format!(
                "Redis is not listening on {host}:{port}. Detected bind is {bind}. Tried via {method}."
            ),
            Self::TimedOut { host, port, method } => {
                format!("Timed out reaching {host}:{port} via {method}.")
            }
            Self::Other(text) => redact(text),
        }
    }
}

pub fn connect_host(host: Option<&str>) -> String {
    match host.unwrap_or("").trim() {
        "" | "localhost" | "::1" | "[::1]" | "*" | "0.0.0.0" | "::" | "[::]" => "127.0.0.1".into(),
        other if other.contains(':') => "127.0.0.1".into(),
        other => other.to_string(),
    }
}

pub async fn fetch(
    session: &SshSession,
    dial: &Dial,
    username: Option<String>,
    password: Option<String>,
) -> Result<Fetched, RedisAccessError> {
    if !dial.tls && dial.docker.is_none() && dial.port > 0 {
        match tunnel_fetch(session, dial, username.as_deref(), password.as_deref()).await {
            Ok(fetched) => return Ok(fetched),
            Err(RedisAccessError::NeedsPassword) => return Err(RedisAccessError::NeedsPassword),
            Err(RedisAccessError::ProtectedMode { .. }) => {}
            Err(_) => {}
        }
    }
    match cli_fetch(session, dial, username.as_deref(), password.as_deref(), false).await {
        Ok(fetched) => Ok(fetched),
        Err(err) => {
            if dial.socket.as_ref().is_some_and(|s| !s.is_empty()) {
                if let Ok(fetched) =
                    cli_fetch(session, dial, username.as_deref(), password.as_deref(), true).await
                {
                    return Ok(fetched);
                }
            }
            Err(err)
        }
    }
}

pub async fn ping(
    session: &SshSession,
    dial: &Dial,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<String, RedisAccessError> {
    if !dial.tls && dial.docker.is_none() {
        match tunnel_command(session, dial, username, password, &["PING"]).await {
            Ok(Resp::Simple(text)) if text.eq_ignore_ascii_case("PONG") => {
                return Ok(format!("PONG from {}:{port} via direct-tcpip.", dial.host, port = dial.port));
            }
            Ok(Resp::Error(text)) => {
                if let Some(err) = classify_text(&text, dial, "direct-tcpip") {
                    if matches!(err, RedisAccessError::NeedsPassword) {
                        return Err(err);
                    }
                }
            }
            Ok(_) => {
                return Ok(format!("Redis answered on {}:{port} via direct-tcpip.", dial.host, port = dial.port));
            }
            Err(RedisAccessError::NeedsPassword) => return Err(RedisAccessError::NeedsPassword),
            Err(_) => {}
        }
    }
    let mut args = RedisCliArgs::new("PING");
    args.host = Some(dial.host.clone());
    args.port = Some(dial.port);
    args.tls = dial.tls;
    if let Some(user) = username {
        args.username = Some(user.to_string());
    }
    let command = build_redis_cli_invocation("redis-cli", &args, password);
    let text = run_cli(session, &command, dial, "redis-cli").await?;
    if text.to_ascii_uppercase().contains("PONG") {
        Ok(format!("PONG from {}:{port} via redis-cli.", dial.host, port = dial.port))
    } else if let Some(err) = classify_text(&text, dial, "redis-cli") {
        Err(err)
    } else {
        Ok(format!("Redis answered on {}:{port} via redis-cli.", dial.host, port = dial.port))
    }
}

async fn tunnel_fetch(
    session: &SshSession,
    dial: &Dial,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<Fetched, RedisAccessError> {
    let commands = handshake_commands(password, username);
    let mut channel = session
        .channel_open_direct_tcpip(&dial.host, u32::from(dial.port), "127.0.0.1", 0)
        .await
        .map_err(|err| open_error(dial, err.to_string()))?;
    let mut info = String::new();
    let mut keyspace = String::new();
    let mut slowlog = Vec::new();
    let mut maxmemory = 0u64;
    for command in &commands {
        let args: Vec<&str> = command.iter().map(String::as_str).collect();
        let value = roundtrip(&mut channel, &encode_command(&args), dial, "direct-tcpip").await?;
        if let Resp::Error(text) = &value {
            return Err(classify_text(text, dial, "direct-tcpip")
                .unwrap_or_else(|| RedisAccessError::Other(text.clone())));
        }
        match command.first().map(String::as_str) {
            Some("INFO") if command.get(1).map(String::as_str) == Some("keyspace") => {
                keyspace = bulk_text(&value).map_err(RedisAccessError::Other)?;
            }
            Some("INFO") => info = bulk_text(&value).map_err(RedisAccessError::Other)?,
            Some("SLOWLOG") => slowlog = slowlog_from_resp(&value).map_err(RedisAccessError::Other)?,
            Some("CONFIG") => {
                maxmemory = maxmemory_from_resp(&value).map_err(RedisAccessError::Other)?
            }
            _ => {}
        }
    }
    let _ = channel.eof().await;
    Ok(Fetched {
        info,
        keyspace,
        slowlog,
        maxmemory,
        method: "direct-tcpip".into(),
    })
}

async fn tunnel_command(
    session: &SshSession,
    dial: &Dial,
    username: Option<&str>,
    password: Option<&str>,
    command: &[&str],
) -> Result<Resp, RedisAccessError> {
    let mut channel = session
        .channel_open_direct_tcpip(&dial.host, u32::from(dial.port), "127.0.0.1", 0)
        .await
        .map_err(|err| open_error(dial, err.to_string()))?;
    if let Some(password) = password.filter(|p| !p.is_empty()) {
        let auth = if let Some(username) = username.filter(|u| !u.is_empty()) {
            encode_command(&["AUTH", username, password])
        } else {
            encode_command(&["AUTH", password])
        };
        let reply = roundtrip(&mut channel, &auth, dial, "direct-tcpip").await?;
        if let Resp::Error(text) = reply {
            return Err(classify_text(&text, dial, "direct-tcpip")
                .unwrap_or_else(|| RedisAccessError::Other(text)));
        }
    }
    let value = roundtrip(&mut channel, &encode_command(command), dial, "direct-tcpip").await?;
    let _ = channel.eof().await;
    Ok(value)
}

async fn roundtrip(
    channel: &mut russh::Channel<russh::client::Msg>,
    payload: &[u8],
    dial: &Dial,
    method: &str,
) -> Result<Resp, RedisAccessError> {
    channel
        .data(payload)
        .await
        .map_err(|err| RedisAccessError::Other(err.to_string()))?;
    let mut parser = RespParser::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    loop {
        if let Some(value) = parser.next().map_err(RedisAccessError::Other)? {
            return Ok(value);
        }
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return Err(RedisAccessError::TimedOut {
                host: dial.host.clone(),
                port: dial.port,
                method: method.into(),
            });
        }
        match tokio::time::timeout(left, channel.wait()).await {
            Err(_) => {
                return Err(RedisAccessError::TimedOut {
                    host: dial.host.clone(),
                    port: dial.port,
                    method: method.into(),
                })
            }
            Ok(Some(ChannelMsg::Data { data })) => parser.push(&data),
            Ok(Some(ChannelMsg::ExtendedData { data, .. })) => parser.push(&data),
            Ok(Some(ChannelMsg::Eof) | None) => {
                return parser
                    .next()
                    .map_err(RedisAccessError::Other)?
                    .ok_or_else(|| RedisAccessError::Other("Redis closed the connection before a full reply.".into()));
            }
            Ok(Some(_)) => {}
        }
    }
}

async fn cli_fetch(
    session: &SshSession,
    dial: &Dial,
    username: Option<&str>,
    password: Option<&str>,
    socket_only: bool,
) -> Result<Fetched, RedisAccessError> {
    let method = if socket_only { "redis-cli socket" } else { "redis-cli" };
    let info = cli_one(session, dial, username, password, "INFO all", socket_only, method).await?;
    let slow = cli_one(session, dial, username, password, "SLOWLOG GET 128", socket_only, method).await?;
    let keyspace = cli_one(session, dial, username, password, "INFO keyspace", socket_only, method).await?;
    let max_raw = cli_one(session, dial, username, password, "CONFIG GET maxmemory", socket_only, method).await?;
    Ok(Fetched {
        info,
        keyspace,
        slowlog: parse_slowlog_get(&slow).unwrap_or_default(),
        maxmemory: parse_config_get_maxmemory(&max_raw).unwrap_or(0),
        method: method.into(),
    })
}

async fn cli_one(
    session: &SshSession,
    dial: &Dial,
    username: Option<&str>,
    password: Option<&str>,
    subcommand: &str,
    socket_only: bool,
    method: &str,
) -> Result<String, RedisAccessError> {
    let mut args = RedisCliArgs::new(subcommand);
    args.tls = dial.tls;
    if let Some(user) = username {
        args.username = Some(user.to_string());
    }
    if socket_only {
        args.socket = dial.socket.clone();
    } else if let Some(container) = dial.docker.clone().filter(|s| !s.is_empty()) {
        args.docker_container = Some(container);
        args.host = Some(dial.host.clone());
        if dial.port > 0 {
            args.port = Some(dial.port);
        }
    } else {
        args.host = Some(dial.host.clone());
        if dial.port > 0 {
            args.port = Some(dial.port);
        }
    }
    let command = build_redis_cli_invocation("redis-cli", &args, password);
    run_cli(session, &command, dial, method).await
}

async fn run_cli(
    session: &SshSession,
    command: &str,
    dial: &Dial,
    method: &str,
) -> Result<String, RedisAccessError> {
    let result = tokio::time::timeout(Duration::from_secs(8), run_command_merged(session, command)).await;
    let text = match result {
        Err(_) => {
            return Err(RedisAccessError::TimedOut {
                host: dial.host.clone(),
                port: dial.port,
                method: method.into(),
            })
        }
        Ok(Err(err)) => err.to_string(),
        Ok(Ok(text)) => text,
    };
    if let Some(err) = classify_text(&text, dial, method) {
        return Err(err);
    }
    if text.trim().is_empty() {
        return Err(RedisAccessError::Other(format!(
            "redis-cli returned no data for {host}:{port}.",
            host = dial.host,
            port = dial.port
        )));
    }
    Ok(text)
}

fn open_error(dial: &Dial, detail: String) -> RedisAccessError {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        RedisAccessError::TimedOut {
            host: dial.host.clone(),
            port: dial.port,
            method: "direct-tcpip".into(),
        }
    } else {
        RedisAccessError::Refused {
            host: dial.host.clone(),
            port: dial.port,
            bind: dial.bind_label.clone(),
            method: "direct-tcpip".into(),
        }
    }
}

pub fn classify_text(text: &str, dial: &Dial, method: &str) -> Option<RedisAccessError> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("noauth") || lower.contains("wrongpass") || lower.contains("authentication required") {
        return Some(RedisAccessError::NeedsPassword);
    }
    if lower.contains("protected mode") || lower.contains("denied redis is running in protected") {
        return Some(RedisAccessError::ProtectedMode {
            host: dial.host.clone(),
            port: dial.port,
        });
    }
    if lower.contains("connection refused") || lower.contains("could not connect") {
        return Some(RedisAccessError::Refused {
            host: dial.host.clone(),
            port: dial.port,
            bind: dial.bind_label.clone(),
            method: method.into(),
        });
    }
    if lower.contains("timed out") || lower.contains("timeout") || lower.contains("exit status 124") {
        return Some(RedisAccessError::TimedOut {
            host: dial.host.clone(),
            port: dial.port,
            method: method.into(),
        });
    }
    if lower.contains("exit status") || lower.contains("command failed") {
        return Some(RedisAccessError::Other(redact(text)));
    }
    None
}

pub fn redact(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(idx) = rest.find("REDISCLI_AUTH=") {
        out.push_str(&rest[..idx]);
        out.push_str("REDISCLI_AUTH=***");
        rest = &rest[idx + "REDISCLI_AUTH=".len()..];
        if rest.starts_with('\'') {
            if let Some(end) = rest[1..].find('\'') {
                rest = &rest[end + 2..];
            }
        } else if let Some(end) = rest.find(char::is_whitespace) {
            rest = &rest[end..];
        } else {
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localhost_and_ipv6_become_ipv4_loopback() {
        assert_eq!(connect_host(Some("localhost")), "127.0.0.1");
        assert_eq!(connect_host(Some("::1")), "127.0.0.1");
        assert_eq!(connect_host(Some("[::1]")), "127.0.0.1");
        assert_eq!(connect_host(None), "127.0.0.1");
        assert_eq!(connect_host(Some("10.0.0.8")), "10.0.0.8");
    }

    #[test]
    fn noauth_and_timeout_messages_name_the_target() {
        let dial = Dial::loopback(6379);
        let auth = classify_text("NOAUTH Authentication required.", &dial, "direct-tcpip").unwrap();
        assert_eq!(auth.phase(), "auth_required");
        assert_eq!(auth.message(), "Redis needs a password.");
        assert!(!auth.message().to_lowercase().contains("secret"));
        let timed = classify_text("exit status 124", &dial, "redis-cli").unwrap();
        assert!(timed.message().contains("127.0.0.1:6379"));
        assert!(timed.message().contains("redis-cli"));
        let refused = classify_text("Could not connect to Redis: Connection refused", &dial, "redis-cli").unwrap();
        assert!(refused.message().contains("not listening"));
        assert!(refused.message().contains("127.0.0.1:6379"));
    }

    #[test]
    fn redact_hides_password() {
        let text = redact("REDISCLI_AUTH='s3cret' redis-cli PING");
        assert!(!text.contains("s3cret"));
        assert!(text.contains("REDISCLI_AUTH=***"));
    }
}

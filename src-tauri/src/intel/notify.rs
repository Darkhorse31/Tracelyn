//! Channel payloads and delivery. Webhooks go out through Windows `curl.exe`.
//! SMTP is a small client on top of the OS TLS stack. Secrets stay in Credential Manager.

use super::logic::clip_chars;
use super::types::{Channel, ChannelConfig};
use base64::{engine::general_purpose::STANDARD, Engine};
use keyring::Entry;
use serde_json::json;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Command, Stdio};
use std::time::Duration;

const SERVICE: &str = "Tracelyn";

fn account(channel_id: &str) -> String {
    format!("intel-channel:{channel_id}")
}

pub fn set_channel_secret(channel_id: &str, value: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE, &account(channel_id)).map_err(|e| e.to_string())?;
    entry.set_password(value).map_err(|e| e.to_string())
}

pub fn get_channel_secret(channel_id: &str) -> Option<String> {
    let entry = Entry::new(SERVICE, &account(channel_id)).ok()?;
    entry.get_password().ok()
}

pub fn delete_channel_secret(channel_id: &str) {
    if let Ok(entry) = Entry::new(SERVICE, &account(channel_id)) {
        let _ = entry.delete_credential();
    }
}

pub fn has_channel_secret(channel_id: &str) -> bool {
    get_channel_secret(channel_id).is_some()
}

#[derive(Debug, Clone)]
pub enum Job {
    Windows { title: String, body: String, path: String },
    Https { url: String, body: String },
    Email {
        host: String,
        port: u16,
        tls: String,
        username: String,
        password: String,
        from: String,
        to: String,
        subject: String,
        body: String,
    },
}

pub fn validate_https_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    if url.len() > 2000 {
        return Err("webhook URL is too long".into());
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("webhook URL cannot contain spaces or line breaks".into());
    }
    if !url.starts_with("https://") {
        return Err("webhook URL must start with https://".into());
    }
    Ok(())
}

pub fn validate_mail(value: &str) -> Result<(), String> {
    if value.len() > 200 || value.contains(['\n', '\r', ' ']) || !value.contains('@') {
        return Err("email address must contain @ and no spaces".into());
    }
    Ok(())
}

pub fn validate_host(host: &str) -> Result<(), String> {
    if host.is_empty() || host.len() > 253 {
        return Err("host is missing".into());
    }
    if !host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':')
    {
        return Err("host has characters that are not allowed".into());
    }
    Ok(())
}

fn slack_body(text: &str) -> String {
    json!({ "text": clip_chars(text, 3000) }).to_string()
}

fn discord_body(text: &str) -> String {
    json!({ "content": clip_chars(text, 1900) }).to_string()
}

fn teams_body(text: &str) -> String {
    json!({ "text": clip_chars(text, 3000) }).to_string()
}

fn webhook_body(title: &str, text: &str) -> String {
    json!({
        "source": "tracelyn",
        "title": clip_chars(title, 200),
        "text": clip_chars(text, 3500)
    })
    .to_string()
}

pub fn build_jobs(channels: &[Channel], title: &str, body: &str, path: &str) -> (Vec<Job>, Vec<String>) {
    let mut jobs = Vec::new();
    let mut errors = Vec::new();
    let text = format!("{title}\n{body}");
    for channel in channels.iter().filter(|c| c.enabled) {
        match channel.kind.as_str() {
            "windows" => jobs.push(Job::Windows {
                title: clip_chars(title, 120),
                body: clip_chars(body, 400),
                path: if path.starts_with('/') { path.to_string() } else { "/alerts".into() },
            }),
            "slack" | "discord" | "teams" | "webhook" => match get_channel_secret(&channel.id) {
                Some(url) => match validate_https_url(&url) {
                    Ok(()) => {
                        let payload = match channel.kind.as_str() {
                            "slack" => slack_body(&text),
                            "discord" => discord_body(&text),
                            "teams" => teams_body(&text),
                            _ => webhook_body(title, body),
                        };
                        jobs.push(Job::Https { url, body: payload });
                    }
                    Err(err) => errors.push(format!("{}: {err}", channel.name)),
                },
                None => errors.push(format!("{}: save a webhook URL first", channel.name)),
            },
            "telegram" => {
                let token = get_channel_secret(&channel.id);
                let chat = channel.config.chat_id.trim();
                match token {
                    Some(token) if !chat.is_empty() && !token.contains([' ', '\n']) => {
                        let url = format!("https://api.telegram.org/bot{token}/sendMessage");
                        let payload = json!({
                            "chat_id": chat,
                            "text": clip_chars(&text, 4000)
                        })
                        .to_string();
                        jobs.push(Job::Https { url, body: payload });
                    }
                    _ => errors.push(format!("{}: bot token and chat id are required", channel.name)),
                }
            }
            "email" => match email_job(&channel.config, &channel.id, title, body) {
                Ok(job) => jobs.push(job),
                Err(err) => errors.push(format!("{}: {err}", channel.name)),
            },
            other => errors.push(format!("{}: unknown channel kind {other}", channel.name)),
        }
    }
    (jobs, errors)
}

fn email_job(config: &ChannelConfig, channel_id: &str, title: &str, body: &str) -> Result<Job, String> {
    validate_host(&config.smtp_host)?;
    validate_mail(&config.mail_from)?;
    validate_mail(&config.mail_to)?;
    if config.smtp_port == 0 {
        return Err("SMTP port is required".into());
    }
    let password = get_channel_secret(channel_id).unwrap_or_default();
    if !config.username.is_empty() && password.is_empty() {
        return Err("save the SMTP password first".into());
    }
    Ok(Job::Email {
        host: config.smtp_host.clone(),
        port: config.smtp_port,
        tls: config.smtp_tls.clone(),
        username: config.username.clone(),
        password,
        from: config.mail_from.clone(),
        to: config.mail_to.clone(),
        subject: clip_chars(title, 180),
        body: clip_chars(body, 8000),
    })
}

pub fn deliver_job(job: &Job, notify_windows: &mut dyn FnMut(&str, &str, &str) -> Result<(), String>) -> Result<(), String> {
    crate::license::require_write()?;
    match job {
        Job::Windows { title, body, path } => notify_windows(title, body, path),
        Job::Https { url, body } => https_post(url, body).map(|_| ()),
        Job::Email {
            host,
            port,
            tls,
            username,
            password,
            from,
            to,
            subject,
            body,
        } => send_smtp(host, *port, tls, username, password, from, to, subject, body),
    }
}

pub fn https_post(url: &str, body: &str) -> Result<u16, String> {
    validate_https_url(url)?;
    if body.len() > 100_000 {
        return Err("payload is too large".into());
    }
    let mut command = Command::new("curl.exe");
    command
        .args([
            "--silent",
            "--show-error",
            "--max-time",
            "20",
            "--proto",
            "=https",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-H",
            "User-Agent: Tracelyn",
            "--data-binary",
            "@-",
            "-w",
            "\n%{http_code}",
            url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|err| {
        format!("could not start curl.exe ({err}). Webhooks need the Windows curl client")
    })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() && output.stdout.is_empty() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("webhook request failed: {}", clip_chars(err.trim(), 180)));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let code = text
        .lines()
        .last()
        .unwrap_or("")
        .trim()
        .parse::<u16>()
        .map_err(|_| format!("webhook returned an unexpected response from {}", host_only(url)))?;
    if !(200..300).contains(&code) {
        return Err(format!("webhook {code} from {}", host_only(url)));
    }
    Ok(code)
}

fn host_only(url: &str) -> &str {
    url.trim_start_matches("https://").split('/').next().unwrap_or("webhook")
}

pub fn smtp_data(from: &str, to: &str, subject: &str, body: &str) -> String {
    let mut stuffed = String::new();
    for line in body.split('\n') {
        if line.starts_with('.') {
            stuffed.push('.');
        }
        stuffed.push_str(line.trim_end_matches('\r'));
        stuffed.push_str("\r\n");
    }
    format!(
        "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{stuffed}.\r\n"
    )
}

fn send_smtp(
    host: &str,
    port: u16,
    tls: &str,
    username: &str,
    password: &str,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    validate_host(host)?;
    let addr = format!("{host}:{port}");
    let socket = addr
        .to_socket_addrs()
        .map_err(|e| format!("SMTP address: {e}"))?
        .next()
        .ok_or_else(|| "SMTP address did not resolve".to_string())?;
    let tcp = TcpStream::connect_timeout(&socket, Duration::from_secs(15))
        .map_err(|e| format!("SMTP connect: {e}"))?;
    tcp.set_read_timeout(Some(Duration::from_secs(20))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(20))).ok();
    if tls == "implicit" {
        let tls_stream = tls_wrap(host, tcp)?;
        smtp_session(tls_stream, host, username, password, from, to, subject, body)
    } else if tls == "none" {
        smtp_session(tcp, host, username, password, from, to, subject, body)
    } else {
        smtp_starttls(tcp, host, username, password, from, to, subject, body)
    }
}

fn tls_wrap(host: &str, tcp: TcpStream) -> Result<native_tls::TlsStream<TcpStream>, String> {
    let connector = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| e.to_string())?;
    connector.connect(host, tcp).map_err(|e| format!("TLS: {e}"))
}

fn smtp_starttls(
    mut tcp: TcpStream,
    host: &str,
    username: &str,
    password: &str,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    read_smtp(&mut tcp)?;
    smtp_cmd(&mut tcp, "EHLO tracelyn")?;
    smtp_cmd(&mut tcp, "STARTTLS")?;
    let mut tls = tls_wrap(host, tcp)?;
    smtp_cmd(&mut tls, "EHLO tracelyn")?;
    smtp_auth(&mut tls, username, password)?;
    smtp_envelope(&mut tls, from, to, subject, body)
}

fn smtp_session<S: Read + Write>(
    mut stream: S,
    _host: &str,
    username: &str,
    password: &str,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    read_smtp(&mut stream)?;
    smtp_cmd(&mut stream, "EHLO tracelyn")?;
    smtp_auth(&mut stream, username, password)?;
    smtp_envelope(&mut stream, from, to, subject, body)
}

fn smtp_auth<S: Read + Write>(stream: &mut S, username: &str, password: &str) -> Result<(), String> {
    if username.is_empty() {
        return Ok(());
    }
    let raw = format!("\0{username}\0{password}");
    let encoded = STANDARD.encode(raw.as_bytes());
    let reply = smtp_cmd(stream, &format!("AUTH PLAIN {encoded}"))?;
    if reply.starts_with('5') {
        return Err("SMTP rejected the credentials".into());
    }
    Ok(())
}

fn smtp_envelope<S: Read + Write>(
    stream: &mut S,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    smtp_cmd(stream, &format!("MAIL FROM:<{from}>"))?;
    smtp_cmd(stream, &format!("RCPT TO:<{to}>"))?;
    smtp_cmd(stream, "DATA")?;
    let data = smtp_data(from, to, subject, body);
    stream.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    let reply = read_smtp(stream)?;
    if !reply.starts_with('2') {
        return Err(format!("SMTP data was rejected ({})", reply.chars().take(3).collect::<String>()));
    }
    let _ = smtp_cmd(stream, "QUIT");
    Ok(())
}

fn smtp_cmd<S: Read + Write>(stream: &mut S, line: &str) -> Result<String, String> {
    stream
        .write_all(format!("{line}\r\n").as_bytes())
        .map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    let reply = read_smtp(stream)?;
    if reply.starts_with('2') || reply.starts_with('3') {
        Ok(reply)
    } else if reply.starts_with("535") {
        Err("SMTP rejected the credentials".into())
    } else {
        Err(format!(
            "SMTP {}",
            reply.lines().next().unwrap_or("error").chars().take(80).collect::<String>()
        ))
    }
}

fn read_smtp<S: Read>(stream: &mut S) -> Result<String, String> {
    let mut all = String::new();
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = stream.read(&mut byte).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("SMTP closed the connection".into());
        }
        line.push(byte[0]);
        if byte[0] == b'\n' {
            let text = String::from_utf8_lossy(&line).to_string();
            let done = text.len() >= 4 && text.as_bytes().get(3) == Some(&b' ');
            all.push_str(&text);
            line.clear();
            if done {
                return Ok(all);
            }
        }
        if all.len() + line.len() > 8_192 {
            return Err("SMTP reply was too long".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_url_rules() {
        assert!(validate_https_url("https://hooks.example/a").is_ok());
        assert!(validate_https_url("http://hooks.example/a").is_err());
        assert!(validate_https_url("https://bad host").is_err());
    }

    #[test]
    fn payloads_are_json_and_clip() {
        let slack: serde_json::Value = serde_json::from_str(&slack_body("say \"hi\"")).unwrap();
        assert_eq!(slack["text"], "say \"hi\"");
        let long = "x".repeat(5000);
        let discord: serde_json::Value = serde_json::from_str(&discord_body(&long)).unwrap();
        assert!(discord["content"].as_str().unwrap().chars().count() <= 1900);
    }

    #[test]
    fn smtp_body_dot_stuffs_and_has_headers() {
        let raw = smtp_data("a@b.co", "c@d.co", "Sub", "hello\n.secret");
        assert!(raw.contains("Subject: Sub\r\n"));
        assert!(raw.contains("\r\n..secret\r\n"));
        assert!(raw.ends_with(".\r\n"));
        assert!(!raw.contains("password"));
    }

    #[test]
    fn windows_job_needs_no_secret() {
        let channel = Channel {
            id: "c1".into(),
            name: "Desktop".into(),
            kind: "windows".into(),
            enabled: true,
            config: ChannelConfig::default(),
            has_secret: false,
            created_at_ms: 0,
        };
        let (jobs, errors) = build_jobs(&[channel], "Title", "Body", "/alerts");
        assert!(errors.is_empty());
        assert!(matches!(jobs[0], Job::Windows { .. }));
    }
}

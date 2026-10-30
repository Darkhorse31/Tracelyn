use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracelyn_core::types::Channel;
#[derive(Clone, Serialize, Deserialize)]
pub struct ServerChannel {
    #[serde(flatten)]
    pub channel: Channel,
    #[serde(default)]
    pub secret: String,
}
pub fn payload(kind: &str, chat: &str, title: &str, body: &str) -> Value {
    let text = tracelyn_core::logic::clip_chars(&format!("{title}\n{body}"), 1900);
    match kind {
        "telegram" => json!({"chat_id":chat,"text":text}),
        "discord" => json!({"content":text}),
        "slack" | "teams" => json!({"text":text}),
        "ntfy" => json!({"title":title,"message":body}),
        _ => json!({"source":"tracelyn","title":title,"text":body}),
    }
}
pub fn send(
    client: &reqwest::blocking::Client,
    entry: &ServerChannel,
    title: &str,
    body: &str,
) -> Result<(), String> {
    let c = &entry.channel;
    if c.kind == "email" {
        use lettre::{transport::smtp::authentication::Credentials, SmtpTransport, Transport};
        let message = email_message(&c.config, title, body)?;
        let builder = match c.config.smtp_tls.as_str() {
            "tls" | "implicit" => SmtpTransport::relay(&c.config.smtp_host),
            "starttls" => SmtpTransport::starttls_relay(&c.config.smtp_host),
            _ => return Err("SMTP requires TLS or STARTTLS".into()),
        }
        .map_err(|_| "Invalid SMTP host")?;
        let mut builder = builder
            .port(c.config.smtp_port)
            .timeout(Some(std::time::Duration::from_secs(10)));
        if !c.config.username.is_empty() {
            builder = builder.credentials(Credentials::new(
                c.config.username.clone(),
                entry.secret.clone(),
            ));
        }
        builder
            .build()
            .send(&message)
            .map_err(|_| "SMTP delivery failed")?;
        return Ok(());
    }
    let url = if c.kind == "telegram" {
        format!("https://api.telegram.org/bot{}/sendMessage", entry.secret)
    } else {
        entry.secret.clone()
    };
    let mut parsed = reqwest::Url::parse(&url).map_err(|_| "Invalid notification URL")?;
    if parsed.scheme() != "https" {
        return Err("Notifications require HTTPS".into());
    }
    let request = if c.kind == "ntfy" {
        let topic = parsed.path().trim_matches('/').to_owned();
        if topic.is_empty() {
            return Err("An ntfy topic URL is required".into());
        }
        parsed.set_path("/");
        let mut message = payload("ntfy", "", title, body);
        message["topic"] = json!(topic);
        client.post(parsed).json(&message)
    } else {
        client
            .post(parsed)
            .json(&payload(&c.kind, &c.config.chat_id, title, body))
    };
    let response = request.send().map_err(|_| "Notification delivery failed")?;
    if !response.status().is_success() {
        return Err("Notification endpoint returned an unsuccessful status".into());
    }
    Ok(())
}
pub fn email_message(
    config: &tracelyn_core::types::ChannelConfig,
    title: &str,
    body: &str,
) -> Result<lettre::Message, String> {
    lettre::Message::builder()
        .from(config.mail_from.parse().map_err(|_| "Invalid sender")?)
        .to(config.mail_to.parse().map_err(|_| "Invalid recipient")?)
        .subject(title)
        .body(body.to_owned())
        .map_err(|_| "Invalid email".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn telegram() {
        assert_eq!(
            payload("telegram", "42", "t", "b"),
            json!({"chat_id":"42","text":"t\nb"})
        );
    }
    #[test]
    fn discord() {
        assert_eq!(payload("discord", "", "t", "b")["content"], "t\nb");
    }
    #[test]
    fn slack() {
        assert_eq!(payload("slack", "", "t", "b")["text"], "t\nb");
    }
    #[test]
    fn webhook() {
        assert_eq!(
            payload("webhook", "", "t", "b"),
            json!({"source":"tracelyn","title":"t","text":"b"})
        );
    }
    #[test]
    fn ntfy() {
        assert_eq!(payload("ntfy", "", "t", "b")["message"], "b");
    }
    #[test]
    fn email_headers_and_body() {
        let c = tracelyn_core::types::ChannelConfig {
            mail_from: "alerts@example.com".into(),
            mail_to: "ops@example.com".into(),
            ..Default::default()
        };
        let text = String::from_utf8(
            email_message(&c, "CPU high", "Value: 95")
                .unwrap()
                .formatted(),
        )
        .unwrap();
        for expected in [
            "From: alerts@example.com",
            "To: ops@example.com",
            "Subject: CPU high",
            "Value: 95",
        ] {
            assert!(text.contains(expected), "{text}");
        }
    }
    #[test]
    fn email_rejects_invalid_recipient() {
        let c = tracelyn_core::types::ChannelConfig {
            mail_from: "alerts@example.com".into(),
            mail_to: "invalid".into(),
            ..Default::default()
        };
        assert!(email_message(&c, "t", "b").is_err());
    }
}

use thiserror::Error;

const SERVICE: &str = "Tracelyn";

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("keyring error: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("secret not found for {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    Password,
    KeyPassphrase,
    RedisPassword,
    RedisUsername,
    SudoPassword,
}

impl SecretKind {
    fn account(server_id: &str, kind: SecretKind) -> String {
        let suffix = match kind {
            SecretKind::Password => "password",
            SecretKind::KeyPassphrase => "key_passphrase",
            SecretKind::RedisPassword => "redis_password",
            SecretKind::RedisUsername => "redis_username",
            SecretKind::SudoPassword => "sudo_password",
        };
        format!("server:{}:{}", server_id, suffix)
    }
}

pub fn set_secret(server_id: &str, kind: SecretKind, value: &str) -> Result<(), SecretError> {
    let entry = keyring::Entry::new(SERVICE, &SecretKind::account(server_id, kind))?;
    entry.set_password(value)?;
    Ok(())
}

pub fn get_secret(server_id: &str, kind: SecretKind) -> Result<String, SecretError> {
    let entry = keyring::Entry::new(SERVICE, &SecretKind::account(server_id, kind))?;
    entry
        .get_password()
        .map_err(|_| SecretError::NotFound(SecretKind::account(server_id, kind)))
}

pub fn delete_secrets(server_id: &str) {
    for kind in [
        SecretKind::Password,
        SecretKind::KeyPassphrase,
        SecretKind::RedisPassword,
        SecretKind::RedisUsername,
        SecretKind::SudoPassword,
    ] {
        if let Ok(entry) = keyring::Entry::new(SERVICE, &SecretKind::account(server_id, kind)) {
            let _ = entry.delete_credential();
        }
    }
}

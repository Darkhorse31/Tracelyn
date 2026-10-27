use russh_keys::key::PublicKey;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyPrompt {
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
    pub changed: bool,
    pub expected: Option<String>,
}

impl std::fmt::Display for HostKeyPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap_or_default())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCheck {
    Trusted,
    Unknown { fingerprint: String },
    Changed { expected: String, actual: String },
}

#[derive(Debug, Error)]
pub enum HostKeyError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("host key changed for {host}:{port} (expected {expected}, got {actual})")]
    Changed {
        host: String,
        port: u16,
        expected: String,
        actual: String,
    },
    #[error("fingerprint must be a SHA256 host key")]
    BadFingerprint,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct KnownHostsFile {
    entries: HashMap<String, String>,
}

pub struct HostKeyStore {
    path: PathBuf,
    data: KnownHostsFile,
}

impl HostKeyStore {
    pub fn load() -> Result<Self, HostKeyError> {
        let path = known_hosts_path();
        let data = if path.exists() {
            let raw = fs::read_to_string(&path)?;
            serde_json::from_str(&raw)?
        } else {
            KnownHostsFile::default()
        };
        Ok(Self { path, data })
    }

    fn save(&self) -> Result<(), HostKeyError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(&self.data)?;
        fs::write(&self.path, raw)?;
        Ok(())
    }

    pub fn inspect(&self, host: &str, port: u16, key: &PublicKey) -> KeyCheck {
        decide(self.data.entries.get(&host_key_id(host, port)).map(String::as_str), &fingerprint(key))
    }

    /// Records a fingerprint the user accepted. Unknown and changed keys stay blocked until this runs.
    pub fn trust(&mut self, host: &str, port: u16, fingerprint: &str) -> Result<(), HostKeyError> {
        let fingerprint = fingerprint.trim();
        if !valid_fingerprint(fingerprint) {
            return Err(HostKeyError::BadFingerprint);
        }
        self.data.entries.insert(host_key_id(host, port), fingerprint.to_string());
        self.save()
    }
}

pub fn decide(stored: Option<&str>, fingerprint: &str) -> KeyCheck {
    match stored {
        None => KeyCheck::Unknown {
            fingerprint: fingerprint.to_string(),
        },
        Some(stored) if same_fingerprint(stored, fingerprint) => KeyCheck::Trusted,
        Some(expected) => KeyCheck::Changed {
            expected: expected.to_string(),
            actual: fingerprint.to_string(),
        },
    }
}

fn valid_fingerprint(value: &str) -> bool {
    let rest = value.strip_prefix("SHA256:").unwrap_or("");
    let body = rest.trim_end_matches('=');
    (43..=50).contains(&body.len())
        && body.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '-' || c == '_')
}

fn same_fingerprint(stored: &str, presented: &str) -> bool {
    fn norm(value: &str) -> String {
        value.trim().trim_start_matches("SHA256:").trim_end_matches('=').to_string()
    }
    norm(stored) == norm(presented)
}

fn host_key_id(host: &str, port: u16) -> String {
    format!("{}:{}", host, port)
}

fn fingerprint(key: &PublicKey) -> String {
    format!("SHA256:{}", key.fingerprint())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_and_changed_keys_are_not_trusted_automatically() {
        assert!(matches!(decide(None, "SHA256:abc"), KeyCheck::Unknown { .. }));
        match decide(Some("SHA256:old"), "SHA256:new") {
            KeyCheck::Changed { expected, actual } => {
                assert_eq!(expected, "SHA256:old");
                assert_eq!(actual, "SHA256:new");
            }
            other => panic!("expected changed, got {other:?}"),
        }
        assert_eq!(decide(Some("SHA256:abc="), "SHA256:abc"), KeyCheck::Trusted);
    }

    #[test]
    fn trust_rejects_a_fingerprint_that_is_not_sha256() {
        let mut store = HostKeyStore {
            path: std::env::temp_dir().join(format!("tracelyn-host-keys-{}.json", uuid::Uuid::new_v4())),
            data: KnownHostsFile::default(),
        };
        assert!(store.trust("example.com", 22, "md5:aa").is_err());
        let fp = format!("SHA256:{}", "A".repeat(43));
        store.trust("example.com", 22, &fp).unwrap();
        assert_eq!(decide(store.data.entries.get("example.com:22").map(String::as_str), &fp), KeyCheck::Trusted);
        let _ = std::fs::remove_file(&store.path);
    }
}

fn known_hosts_path() -> PathBuf {
    let base = dirs::data_local_dir().expect("local data dir");
    base.join("Tracelyn").join("known_hosts.json")
}

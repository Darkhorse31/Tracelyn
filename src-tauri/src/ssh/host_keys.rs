use russh_keys::key::PublicKey;
use russh_keys::PublicKeyBase64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

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

    pub fn verify_or_trust(
        &mut self,
        host: &str,
        port: u16,
        key: &PublicKey,
    ) -> Result<(), HostKeyError> {
        let id = host_key_id(host, port);
        let fingerprint = fingerprint(key);
        match self.data.entries.get(&id) {
            None => {
                self.data.entries.insert(id, fingerprint);
                self.save()?;
                Ok(())
            }
            Some(stored) if stored == &fingerprint => Ok(()),
            Some(expected) => Err(HostKeyError::Changed {
                host: host.to_string(),
                port,
                expected: expected.clone(),
                actual: fingerprint,
            }),
        }
    }
}

fn host_key_id(host: &str, port: u16) -> String {
    format!("{}:{}", host, port)
}

fn fingerprint(key: &PublicKey) -> String {
    let blob = key.public_key_bytes();
    let digest = Sha256::digest(blob);
    format!("SHA256:{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, digest))
}

fn known_hosts_path() -> PathBuf {
    let base = dirs::data_local_dir().expect("local data dir");
    base.join("Tracelyn").join("known_hosts.json")
}

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInspection {
    pub ok: bool,
    pub message: String,
    pub putty: bool,
    pub encrypted: bool,
    pub needs_passphrase: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyList {
    pub directory: String,
    pub keys: Vec<String>,
}

pub fn ssh_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh")
}

pub fn detected_private_keys() -> Vec<String> {
    let mut keys = list_private_keys(&ssh_dir());
    if let Some(docs) = dirs::document_dir() {
        keys.extend(list_ppk_files(&docs));
    }
    keys.sort();
    keys.dedup();
    keys
}

pub fn list_private_keys(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file() && is_key_candidate(p))
            .filter_map(|p| p.to_str().map(|s| s.to_string()))
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

fn list_ppk_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file() && is_ppk_name(p))
            .filter_map(|p| p.to_str().map(|s| s.to_string()))
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

fn is_ppk_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().ends_with(".ppk"))
}

fn is_key_candidate(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".pub") || lower == "known_hosts" || lower == "config" || lower.starts_with("known_hosts")
    {
        return false;
    }
    matches!(lower.as_str(), "id_ed25519" | "id_rsa" | "id_ecdsa")
        || lower.ends_with(".pem")
        || lower.ends_with(".ppk")
}

pub fn inspect_key_file(path: &Path, passphrase: Option<&str>) -> KeyInspection {
    if !path.exists() {
        return bad("That file does not exist. Check the path, or browse to the private key.");
    }
    if path.is_dir() {
        return bad("That path is a folder. Choose the private key file inside it.");
    }
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(_) => {
            return bad("Tracelyn cannot read that file. Check that your Windows user can open it.");
        }
    };
    inspect_key_bytes(&bytes, path, passphrase)
}

fn inspect_key_bytes(bytes: &[u8], path: &Path, passphrase: Option<&str>) -> KeyInspection {
    if super::ppk::is_ppk_bytes(bytes) {
        return inspect_ppk(bytes, passphrase);
    }
    match classify_key(path, bytes) {
        Ok(()) => KeyInspection {
            ok: true,
            message: "OpenSSH private key.".into(),
            putty: false,
            encrypted: false,
            needs_passphrase: false,
        },
        Err(message) => bad(&message),
    }
}

fn inspect_ppk(bytes: &[u8], passphrase: Option<&str>) -> KeyInspection {
    let encrypted = super::ppk::is_encrypted(bytes);
    match super::ppk::load_key_pair(bytes, passphrase) {
        Ok(_) => KeyInspection {
            ok: true,
            message: "PuTTY private key.".into(),
            putty: true,
            encrypted,
            needs_passphrase: false,
        },
        Err(super::ppk::PpkError::NeedsPassphrase) => KeyInspection {
            ok: false,
            message: super::ppk::PpkError::NeedsPassphrase.to_string(),
            putty: true,
            encrypted: true,
            needs_passphrase: true,
        },
        Err(err) => KeyInspection {
            ok: false,
            message: err.to_string(),
            putty: true,
            encrypted,
            needs_passphrase: false,
        },
    }
}

pub fn classify_key(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim_start();
    if name.ends_with(".pub") || looks_like_public_key(trimmed) {
        return Err(
            "This is a public key (.pub). Choose the matching private key, the file without .pub."
                .into(),
        );
    }
    if super::ppk::is_ppk_bytes(bytes) {
        return match super::ppk::load_key_pair(bytes, None) {
            Ok(_) => Ok(()),
            Err(super::ppk::PpkError::NeedsPassphrase) => Ok(()),
            Err(err) => Err(err.to_string()),
        };
    }
    if looks_like_openssh_private(trimmed) {
        return Ok(());
    }
    Err(
        "This file is not a readable private key. Use an OpenSSH key beginning with -----BEGIN OPENSSH PRIVATE KEY----- or -----BEGIN RSA PRIVATE KEY-----, or a PuTTY .ppk file."
            .into(),
    )
}

fn looks_like_public_key(text: &str) -> bool {
    let head = text.lines().next().unwrap_or(text);
    head.starts_with("ssh-ed25519 ")
        || head.starts_with("ssh-rsa ")
        || head.starts_with("ssh-dss ")
        || head.starts_with("ecdsa-sha2-")
        || head.contains("BEGIN PUBLIC KEY")
        || head.contains("BEGIN SSH2 PUBLIC KEY")
}

fn looks_like_openssh_private(text: &str) -> bool {
    let head: String = text.chars().take(160).collect();
    head.contains("PRIVATE KEY")
}

fn bad(message: &str) -> KeyInspection {
    KeyInspection {
        ok: false,
        message: message.into(),
        putty: false,
        encrypted: false,
        needs_passphrase: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_openssh_and_ppk_and_rejects_pub() {
        let key = Path::new("id_ed25519");
        assert!(classify_key(key, b"-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n").is_ok());
        assert!(classify_key(key, b"-----BEGIN RSA PRIVATE KEY-----\nAAAA\n").is_ok());
        let err = classify_key(Path::new("id_ed25519.pub"), b"ssh-ed25519 AAAAC3 comment").unwrap_err();
        assert!(err.contains("public key"));
        let err = classify_key(Path::new("work.ppk"), b"PuTTY-User-Key-File-3: ssh-ed25519\n").unwrap_err();
        assert!(err.contains("corrupted"));
        let err = classify_key(key, b"just a note").unwrap_err();
        assert!(err.contains("OpenSSH"));
        assert!(err.contains(".ppk"));
    }

    #[test]
    fn documents_listing_keeps_ppk_only() {
        let dir = std::env::temp_dir().join(format!("tracelyn-ppk-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.ppk"), b"not a real key").unwrap();
        fs::write(dir.join("b.PPK"), b"x").unwrap();
        fs::write(dir.join("c.pem"), b"x").unwrap();
        fs::write(dir.join("id_ed25519"), b"x").unwrap();
        let found = list_ppk_files(&dir);
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|p| p.to_ascii_lowercase().ends_with(".ppk")));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn candidate_filter_skips_public_and_ssh_config() {
        assert!(is_key_candidate(Path::new(r"C:\Users\a\.ssh\id_ed25519")));
        assert!(is_key_candidate(Path::new(r"C:\Users\a\.ssh\id_rsa")));
        assert!(is_key_candidate(Path::new(r"C:\Users\a\.ssh\id_ecdsa")));
        assert!(is_key_candidate(Path::new(r"C:\Users\a\.ssh\deploy.pem")));
        assert!(is_key_candidate(Path::new(r"C:\Users\a\.ssh\old.ppk")));
        assert!(!is_key_candidate(Path::new(r"C:\Users\a\.ssh\id_ed25519.pub")));
        assert!(!is_key_candidate(Path::new(r"C:\Users\a\.ssh\known_hosts")));
        assert!(!is_key_candidate(Path::new(r"C:\Users\a\.ssh\config")));
        assert!(!is_key_candidate(Path::new(r"C:\Users\a\.ssh\notes.txt")));
    }
}

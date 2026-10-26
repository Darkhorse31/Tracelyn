use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInspection {
    pub ok: bool,
    pub message: String,
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

pub fn inspect_key_file(path: &Path) -> KeyInspection {
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
    match classify_key(path, &bytes) {
        Ok(()) => KeyInspection {
            ok: true,
            message: "OpenSSH private key.".into(),
        },
        Err(message) => bad(&message),
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
    if looks_like_openssh_private(trimmed) {
        return Ok(());
    }
    if name.ends_with(".ppk") || looks_like_putty(trimmed) {
        return Err(
            "This is a PuTTY private key (.ppk). Tracelyn needs an OpenSSH key. Open PuTTYgen, load this key, then use Conversions → Export OpenSSH key and choose that exported file."
                .into(),
        );
    }
    Err(
        "This file is not a readable OpenSSH private key. It should begin with -----BEGIN OPENSSH PRIVATE KEY----- or -----BEGIN RSA PRIVATE KEY-----."
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

fn looks_like_putty(text: &str) -> bool {
    text.starts_with("PuTTY-User-Key-File-")
}

fn bad(message: &str) -> KeyInspection {
    KeyInspection {
        ok: false,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_openssh_and_rejects_pub_and_ppk() {
        let key = Path::new("id_ed25519");
        assert!(classify_key(key, b"-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n").is_ok());
        assert!(classify_key(key, b"-----BEGIN RSA PRIVATE KEY-----\nAAAA\n").is_ok());
        let err = classify_key(Path::new("id_ed25519.pub"), b"ssh-ed25519 AAAAC3 comment").unwrap_err();
        assert!(err.contains("public key"));
        let err = classify_key(Path::new("work.ppk"), b"PuTTY-User-Key-File-3: ssh-ed25519\n").unwrap_err();
        assert!(err.contains("PuTTYgen"));
        assert!(classify_key(key, b"just a note").unwrap_err().contains("OpenSSH"));
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

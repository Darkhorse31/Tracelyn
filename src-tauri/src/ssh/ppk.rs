//! PuTTY .ppk v2 and v3, decrypted only in memory.
//!
//! v2: SHA-1(uint32 counter || passphrase) for the AES-256 key, zero IV,
//! HMAC-SHA1 with SHA-1("putty-private-key-file-mac-key" || passphrase).
//! v3: Argon2id/i/d output of 80 bytes (key, IV, HMAC-SHA256 key).
//! Unencrypted v3 uses an empty MAC key. The MAC covers the padded plaintext.

use std::borrow::Cow;

use aes::Aes256;
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use cbc::Decryptor;
use cbc::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
use ed25519_dalek::SigningKey;
use hmac::{Hmac, Mac};
use russh_keys::key::{KeyPair, SignatureHash};
use sha1::{Digest, Sha1};
use sha2::Sha256;

// ponytail: 256 MiB / 1024 passes / 8 lanes. Raise if a real key needs more.
const MAX_ARGON2_KIB: u32 = 256 * 1024;
const MAX_ARGON2_PASSES: u32 = 1024;
const MAX_ARGON2_LANES: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpkError {
    Corrupt,
    UnsupportedAlgorithm,
    UnsupportedVersion,
    UnsupportedCipher,
    UnsupportedKdf,
    WrongPassphrase,
    NeedsPassphrase,
    TooMuchMemory,
}

impl std::fmt::Display for PpkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Corrupt => "This PuTTY key file is corrupted.",
            Self::UnsupportedAlgorithm => {
                "This PuTTY key uses an unsupported algorithm. Tracelyn supports RSA, Ed25519, and ECDSA."
            }
            Self::UnsupportedVersion => {
                "This PuTTY key version is unsupported. Tracelyn reads PPK version 2 and version 3."
            }
            Self::UnsupportedCipher => "This PuTTY key uses an unsupported cipher.",
            Self::UnsupportedKdf => "This PuTTY key uses an unsupported key derivation.",
            Self::WrongPassphrase => "Wrong passphrase for this PuTTY key.",
            Self::NeedsPassphrase => "This PuTTY key is encrypted. Enter the passphrase.",
            Self::TooMuchMemory => "This PuTTY key asks for too much memory to unlock.",
        })
    }
}

pub fn is_ppk_bytes(bytes: &[u8]) -> bool {
    skip_bom(bytes).starts_with(b"PuTTY-User-Key-File-")
}

pub fn is_encrypted(bytes: &[u8]) -> bool {
    parse(bytes).is_ok_and(|parsed| parsed.encrypted)
}

pub fn load_key_pair(bytes: &[u8], passphrase: Option<&str>) -> Result<KeyPair, PpkError> {
    let parsed = parse(bytes)?;
    if !supported_algorithm(parsed.algorithm) {
        return Err(PpkError::UnsupportedAlgorithm);
    }
    if parsed.encrypted {
        match passphrase {
            Some(p) if !p.is_empty() => {}
            _ => return Err(PpkError::NeedsPassphrase),
        }
    }
    let pass = if parsed.encrypted {
        passphrase.unwrap_or("").as_bytes()
    } else {
        b""
    };
    let mut private = parsed.private.clone();
    let mut mac_key = if parsed.encrypted {
        let (mut key, mut iv, mackey) = derive(&parsed, pass)?;
        private = aes256_cbc_decrypt(&key, &iv, &parsed.private)?;
        wipe(&mut key);
        wipe(&mut iv);
        mackey
    } else {
        derive_mac_key(&parsed, pass)?
    };
    check_mac(&parsed, &private, &mac_key)?;
    wipe(&mut mac_key);
    let pair = key_pair(&parsed, &private)?;
    wipe(&mut private);
    Ok(pair)
}

struct Parsed<'a> {
    version: u8,
    algorithm: &'a [u8],
    encryption: &'a [u8],
    comment: &'a [u8],
    public_blob: Vec<u8>,
    private: Vec<u8>,
    mac_hex: &'a [u8],
    encrypted: bool,
    kdf: Option<Kdf<'a>>,
}

struct Kdf<'a> {
    flavour: &'a [u8],
    memory_kib: u32,
    passes: u32,
    lanes: u32,
    salt: Vec<u8>,
}

fn parse(bytes: &[u8]) -> Result<Parsed<'_>, PpkError> {
    let bytes = skip_bom(bytes);
    let mut lines = Lines::new(bytes);
    let (name, algorithm) = lines.header()?;
    let version = if name == b"PuTTY-User-Key-File-2" {
        2
    } else if name == b"PuTTY-User-Key-File-3" {
        3
    } else if name.starts_with(b"PuTTY-User-Key-File-") {
        return Err(PpkError::UnsupportedVersion);
    } else {
        return Err(PpkError::Corrupt);
    };
    let encryption = lines.expect(b"Encryption")?;
    let encrypted = if encryption == b"aes256-cbc" {
        true
    } else if encryption == b"none" {
        false
    } else {
        return Err(PpkError::UnsupportedCipher);
    };
    let comment = lines.expect(b"Comment")?;
    let public_n = parse_count(lines.expect(b"Public-Lines")?)?;
    let public_blob = lines.blob(public_n)?;
    let kdf = if version >= 3 && encrypted {
        Some(read_kdf(&mut lines)?)
    } else {
        None
    };
    let private_n = parse_count(lines.expect(b"Private-Lines")?)?;
    let private = lines.blob(private_n)?;
    if encrypted && (private.is_empty() || private.len() % 16 != 0) {
        return Err(PpkError::Corrupt);
    }
    let mac_name = {
        let (n, v) = lines.header()?;
        if n != b"Private-MAC" {
            return Err(PpkError::Corrupt);
        }
        v
    };
    Ok(Parsed {
        version,
        algorithm,
        encryption,
        comment,
        public_blob,
        private,
        mac_hex: trim_ascii(mac_name),
        encrypted,
        kdf,
    })
}

fn read_kdf<'a>(lines: &mut Lines<'a>) -> Result<Kdf<'a>, PpkError> {
    let flavour = lines.expect(b"Key-Derivation")?;
    if flavour != b"Argon2id" && flavour != b"Argon2i" && flavour != b"Argon2d" {
        return Err(PpkError::UnsupportedKdf);
    }
    let memory_kib = parse_u32(lines.expect(b"Argon2-Memory")?)?;
    let passes = parse_u32(lines.expect(b"Argon2-Passes")?)?;
    let lanes = parse_u32(lines.expect(b"Argon2-Parallelism")?)?;
    let salt_hex = lines.expect(b"Argon2-Salt")?;
    let salt = hex::decode(trim_ascii(salt_hex)).map_err(|_| PpkError::Corrupt)?;
    if salt.len() < 8 || salt.len() > 256 {
        return Err(PpkError::Corrupt);
    }
    if memory_kib > MAX_ARGON2_KIB || passes > MAX_ARGON2_PASSES || lanes > MAX_ARGON2_LANES {
        return Err(PpkError::TooMuchMemory);
    }
    Ok(Kdf {
        flavour,
        memory_kib,
        passes,
        lanes,
        salt,
    })
}

fn supported_algorithm(alg: &[u8]) -> bool {
    matches!(
        alg,
        b"ssh-rsa"
            | b"ssh-ed25519"
            | b"ecdsa-sha2-nistp256"
            | b"ecdsa-sha2-nistp384"
            | b"ecdsa-sha2-nistp521"
    )
}

fn derive(parsed: &Parsed<'_>, pass: &[u8]) -> Result<([u8; 32], [u8; 16], Vec<u8>), PpkError> {
    if parsed.version <= 2 {
        let mut key = [0u8; 32];
        let mut filled = 0;
        let mut ctr = 0u32;
        while filled < 32 {
            let mut hasher = Sha1::new();
            hasher.update(ctr.to_be_bytes());
            hasher.update(pass);
            let dig = hasher.finalize();
            let n = (32 - filled).min(dig.len());
            key[filled..filled + n].copy_from_slice(&dig[..n]);
            filled += n;
            ctr += 1;
        }
        Ok((key, [0u8; 16], derive_mac_key(parsed, pass)?))
    } else {
        let raw = argon_tag(parsed, pass)?;
        let mut key = [0u8; 32];
        let mut iv = [0u8; 16];
        key.copy_from_slice(&raw[..32]);
        iv.copy_from_slice(&raw[32..48]);
        Ok((key, iv, raw[48..].to_vec()))
    }
}

fn derive_mac_key(parsed: &Parsed<'_>, pass: &[u8]) -> Result<Vec<u8>, PpkError> {
    if parsed.version <= 2 {
        let mut hasher = Sha1::new();
        hasher.update(b"putty-private-key-file-mac-key");
        hasher.update(pass);
        Ok(hasher.finalize().to_vec())
    } else if parsed.encrypted {
        Ok(argon_tag(parsed, pass)?[48..].to_vec())
    } else {
        Ok(Vec::new())
    }
}

fn argon_tag(parsed: &Parsed<'_>, pass: &[u8]) -> Result<Vec<u8>, PpkError> {
    let kdf = parsed.kdf.as_ref().ok_or(PpkError::Corrupt)?;
    let flavour = match kdf.flavour {
        b"Argon2id" => Algorithm::Argon2id,
        b"Argon2i" => Algorithm::Argon2i,
        b"Argon2d" => Algorithm::Argon2d,
        _ => return Err(PpkError::UnsupportedKdf),
    };
    let params = Params::new(kdf.memory_kib, kdf.passes, kdf.lanes, Some(80))
        .map_err(|_| PpkError::Corrupt)?;
    let argon = Argon2::new(flavour, Version::V0x13, params);
    let mut out = vec![0u8; 80];
    argon
        .hash_password_into(pass, &kdf.salt, &mut out)
        .map_err(|_| PpkError::Corrupt)?;
    Ok(out)
}

fn aes256_cbc_decrypt(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, PpkError> {
    let mut buf = data.to_vec();
    let plain = Decryptor::<Aes256>::new_from_slices(key, iv)
        .map_err(|_| PpkError::Corrupt)?
        .decrypt_padded_mut::<NoPadding>(&mut buf)
        .map_err(|_| PpkError::Corrupt)?;
    Ok(plain.to_vec())
}

fn check_mac(parsed: &Parsed<'_>, private_plain: &[u8], mac_key: &[u8]) -> Result<(), PpkError> {
    let expected = hex::decode(parsed.mac_hex).map_err(|_| PpkError::Corrupt)?;
    let want = if parsed.version <= 2 { 20 } else { 32 };
    if expected.len() != want {
        return Err(PpkError::Corrupt);
    }
    let data = mac_preimage(parsed, private_plain);
    let ok = if parsed.version <= 2 {
        verify_sha1(mac_key, &data, &expected)
    } else {
        verify_sha256(mac_key, &data, &expected)
    };
    if ok {
        Ok(())
    } else if parsed.encrypted {
        Err(PpkError::WrongPassphrase)
    } else {
        Err(PpkError::Corrupt)
    }
}

fn verify_sha1(key: &[u8], data: &[u8], expected: &[u8]) -> bool {
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(key) else {
        return false;
    };
    mac.update(data);
    mac.verify_slice(expected).is_ok()
}

fn verify_sha256(key: &[u8], data: &[u8], expected: &[u8]) -> bool {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) else {
        return false;
    };
    mac.update(data);
    mac.verify_slice(expected).is_ok()
}

fn mac_preimage(parsed: &Parsed<'_>, private_plain: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    push_string(&mut out, parsed.algorithm);
    push_string(&mut out, parsed.encryption);
    push_string(&mut out, parsed.comment);
    push_string(&mut out, &parsed.public_blob);
    push_string(&mut out, private_plain);
    out
}

fn key_pair(parsed: &Parsed<'_>, private_plain: &[u8]) -> Result<KeyPair, PpkError> {
    match parsed.algorithm {
        b"ssh-ed25519" => ed25519_pair(&parsed.public_blob, private_plain),
        b"ssh-rsa" => rsa_pair(&parsed.public_blob, private_plain),
        b"ecdsa-sha2-nistp256" => ecdsa_pair(parsed.algorithm, b"nistp256", 32, &parsed.public_blob, private_plain),
        b"ecdsa-sha2-nistp384" => ecdsa_pair(parsed.algorithm, b"nistp384", 48, &parsed.public_blob, private_plain),
        b"ecdsa-sha2-nistp521" => ecdsa_pair(parsed.algorithm, b"nistp521", 66, &parsed.public_blob, private_plain),
        _ => Err(PpkError::UnsupportedAlgorithm),
    }
}

fn ed25519_pair(public_blob: &[u8], private_plain: &[u8]) -> Result<KeyPair, PpkError> {
    let mut pub_r = Cursor::new(public_blob);
    expect_bytes(&mut pub_r, b"ssh-ed25519")?;
    let public = pub_r.string()?;
    if public.len() != 32 || pub_r.rest() != 0 {
        return Err(PpkError::Corrupt);
    }
    let mut priv_r = Cursor::new(private_plain);
    let seed = priv_r.string()?;
    if seed.len() != 32 {
        return Err(PpkError::Corrupt);
    }
    let seed: [u8; 32] = seed.try_into().map_err(|_| PpkError::Corrupt)?;
    let signing = SigningKey::from_bytes(&seed);
    if signing.verifying_key().as_bytes().as_slice() != public {
        return Err(PpkError::Corrupt);
    }
    Ok(KeyPair::Ed25519(signing))
}

fn rsa_pair(public_blob: &[u8], private_plain: &[u8]) -> Result<KeyPair, PpkError> {
    let mut pub_r = Cursor::new(public_blob);
    expect_bytes(&mut pub_r, b"ssh-rsa")?;
    let e = pub_r.mpint()?;
    let n = pub_r.mpint()?;
    if pub_r.rest() != 0 {
        return Err(PpkError::Corrupt);
    }
    let mut priv_r = Cursor::new(private_plain);
    let d = priv_r.mpint()?;
    let p = priv_r.mpint()?;
    let q = priv_r.mpint()?;
    let iqmp = priv_r.mpint()?;
    russh_keys::key::KeyPair::new_rsa_with_hash(
        &russh_keys::protocol::RsaPrivateKey {
            public_key: russh_keys::protocol::RsaPublicKey {
                public_exponent: Cow::Owned(e),
                modulus: Cow::Owned(n),
            },
            private_exponent: Cow::Owned(d),
            prime1: Cow::Owned(p),
            prime2: Cow::Owned(q),
            coefficient: Cow::Owned(iqmp),
            comment: Cow::Owned(Vec::new()),
        },
        None,
        SignatureHash::SHA2_512,
    )
    .map_err(|_| PpkError::Corrupt)
}

fn ecdsa_pair(
    algorithm: &[u8],
    curve: &[u8],
    scalar_len: usize,
    public_blob: &[u8],
    private_plain: &[u8],
) -> Result<KeyPair, PpkError> {
    let mut pub_r = Cursor::new(public_blob);
    expect_bytes(&mut pub_r, algorithm)?;
    expect_bytes(&mut pub_r, curve)?;
    let point = pub_r.string()?.to_vec();
    if pub_r.rest() != 0 {
        return Err(PpkError::Corrupt);
    }
    let mut priv_r = Cursor::new(private_plain);
    let scalar = left_pad(&priv_r.mpint()?, scalar_len)?;
    let key = russh_keys::ec::PrivateKey::new_from_secret_scalar(algorithm, &scalar)
        .map_err(|_| PpkError::Corrupt)?;
    let got = key.to_public_key().to_sec1_bytes();
    if got != point {
        return Err(PpkError::Corrupt);
    }
    Ok(KeyPair::EC { key })
}

fn left_pad(bytes: &[u8], len: usize) -> Result<Vec<u8>, PpkError> {
    if bytes.len() > len {
        return Err(PpkError::Corrupt);
    }
    let mut out = vec![0u8; len - bytes.len()];
    out.extend_from_slice(bytes);
    Ok(out)
}

fn expect_bytes(cur: &mut Cursor<'_>, want: &[u8]) -> Result<(), PpkError> {
    let got = cur.string()?;
    if got != want {
        Err(PpkError::Corrupt)
    } else {
        Ok(())
    }
}

fn push_string(out: &mut Vec<u8>, data: &[u8]) {
    let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(data);
}

struct Lines<'a> {
    lines: Vec<&'a [u8]>,
    i: usize,
}

impl<'a> Lines<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        let lines = bytes
            .split(|b| *b == b'\n')
            .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
            .collect();
        Self { lines, i: 0 }
    }

    fn header(&mut self) -> Result<(&'a [u8], &'a [u8]), PpkError> {
        let line = *self.lines.get(self.i).ok_or(PpkError::Corrupt)?;
        self.i += 1;
        let pos = line.iter().position(|b| *b == b':').ok_or(PpkError::Corrupt)?;
        if pos == 0 {
            return Err(PpkError::Corrupt);
        }
        let mut value = &line[pos + 1..];
        if value.first() == Some(&b' ') {
            value = &value[1..];
        }
        Ok((&line[..pos], value))
    }

    fn expect(&mut self, name: &[u8]) -> Result<&'a [u8], PpkError> {
        let (got, value) = self.header()?;
        if got != name {
            Err(PpkError::Corrupt)
        } else {
            Ok(value)
        }
    }

    fn blob(&mut self, n: usize) -> Result<Vec<u8>, PpkError> {
        if n > 10_000 {
            return Err(PpkError::Corrupt);
        }
        let mut b64 = Vec::new();
        for _ in 0..n {
            let line = *self.lines.get(self.i).ok_or(PpkError::Corrupt)?;
            self.i += 1;
            b64.extend_from_slice(trim_ascii(line));
        }
        if b64.len() > 1_048_576 {
            return Err(PpkError::Corrupt);
        }
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|_| PpkError::Corrupt)
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, i: 0 }
    }

    fn rest(&self) -> usize {
        self.data.len().saturating_sub(self.i)
    }

    fn string(&mut self) -> Result<&'a [u8], PpkError> {
        if self.rest() < 4 {
            return Err(PpkError::Corrupt);
        }
        let mut lenb = [0u8; 4];
        lenb.copy_from_slice(&self.data[self.i..self.i + 4]);
        self.i += 4;
        let n = u32::from_be_bytes(lenb) as usize;
        if n > self.rest() || n > 1_048_576 {
            return Err(PpkError::Corrupt);
        }
        let s = &self.data[self.i..self.i + n];
        self.i += n;
        Ok(s)
    }

    fn mpint(&mut self) -> Result<Vec<u8>, PpkError> {
        let raw = self.string()?;
        if raw.first().is_some_and(|b| b & 0x80 != 0) {
            return Err(PpkError::Corrupt);
        }
        let raw = if raw.first() == Some(&0) { &raw[1..] } else { raw };
        if raw.is_empty() {
            Ok(vec![0])
        } else {
            Ok(raw.to_vec())
        }
    }
}

fn parse_count(v: &[u8]) -> Result<usize, PpkError> {
    let n = parse_u32(v)?;
    Ok(n as usize)
}

fn parse_u32(v: &[u8]) -> Result<u32, PpkError> {
    let s = std::str::from_utf8(trim_ascii(v)).map_err(|_| PpkError::Corrupt)?;
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(PpkError::Corrupt);
    }
    s.parse().map_err(|_| PpkError::Corrupt)
}

fn trim_ascii(line: &[u8]) -> &[u8] {
    let start = line.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(line.len());
    let end = line.iter().rposition(|c| !c.is_ascii_whitespace()).map(|i| i + 1).unwrap_or(0);
    if start >= end {
        &[]
    } else {
        &line[start..end]
    }
}

fn skip_bom(bytes: &[u8]) -> &[u8] {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let start = bytes.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(bytes.len());
    &bytes[start..]
}

fn wipe(buf: &mut [u8]) {
    for b in buf {
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbc::cipher::{block_padding::NoPadding, BlockEncryptMut, KeyIvInit};
    use sha2::Sha256;

    fn push_mpint(out: &mut Vec<u8>, bytes: &[u8]) {
        let mut b = bytes;
        while b.len() > 1 && b[0] == 0 {
            b = &b[1..];
        }
        if b.first().is_some_and(|x| x & 0x80 != 0) {
            push_string(out, &{
                let mut v = vec![0u8];
                v.extend_from_slice(b);
                v
            });
        } else if b.is_empty() {
            push_string(out, &[]);
        } else {
            push_string(out, b);
        }
    }

    fn b64_lines(data: &[u8]) -> (usize, String) {
        let text = base64::engine::general_purpose::STANDARD.encode(data);
        let lines: Vec<&str> = text.as_bytes().chunks(64).map(|c| std::str::from_utf8(c).unwrap()).collect();
        (lines.len(), lines.join("\n"))
    }

    fn pad_private(data: &[u8], encrypted: bool) -> Vec<u8> {
        if !encrypted {
            return data.to_vec();
        }
        let padded_len = {
            let len = data.len() + 15;
            len - (len % 16)
        };
        let mut out = vec![0u8; padded_len];
        out[..data.len()].copy_from_slice(data);
        let hash = Sha1::digest(data);
        let pad = padded_len - data.len();
        if pad > 0 {
            out[data.len()..].copy_from_slice(&hash[..pad]);
        }
        out
    }

    struct Argon {
        flavour: &'static str,
        memory: u32,
        passes: u32,
        lanes: u32,
        salt: &'static [u8],
    }

    fn seal(
        version: u8,
        algorithm: &str,
        comment: &str,
        public_blob: &[u8],
        private_plain: &[u8],
        passphrase: Option<&str>,
        argon: Option<&Argon>,
    ) -> String {
        let encrypted = passphrase.is_some();
        let encryption = if encrypted { "aes256-cbc" } else { "none" };
        let pass = passphrase.unwrap_or("").as_bytes();
        let padded = pad_private(private_plain, encrypted);
        let (key, iv, mac_key) = if version <= 2 {
            let mut material = [0u8; 40];
            for ctr in 0..2u32 {
                let dig = Sha1::digest([ctr.to_be_bytes().as_slice(), pass].concat());
                material[ctr as usize * 20..ctr as usize * 20 + 20].copy_from_slice(&dig);
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&material[..32]);
            let mac_key = Sha1::digest([b"putty-private-key-file-mac-key".as_slice(), pass].concat()).to_vec();
            (key, [0u8; 16], mac_key)
        } else if encrypted {
            let spec = argon.expect("v3 encrypted");
            let flavour = match spec.flavour {
                "Argon2i" => Algorithm::Argon2i,
                "Argon2d" => Algorithm::Argon2d,
                _ => Algorithm::Argon2id,
            };
            let params = Params::new(spec.memory, spec.passes, spec.lanes, Some(80)).unwrap();
            let mut out = [0u8; 80];
            Argon2::new(flavour, Version::V0x13, params)
                .hash_password_into(pass, spec.salt, &mut out)
                .unwrap();
            let mut key = [0u8; 32];
            let mut iv = [0u8; 16];
            key.copy_from_slice(&out[..32]);
            iv.copy_from_slice(&out[32..48]);
            (key, iv, out[48..].to_vec())
        } else {
            ([0u8; 32], [0u8; 16], Vec::new())
        };
        let mut mac_data = Vec::new();
        push_string(&mut mac_data, algorithm.as_bytes());
        push_string(&mut mac_data, encryption.as_bytes());
        push_string(&mut mac_data, comment.as_bytes());
        push_string(&mut mac_data, public_blob);
        push_string(&mut mac_data, &padded);
        let mac = if version <= 2 {
            let mut h = Hmac::<Sha1>::new_from_slice(&mac_key).unwrap();
            h.update(&mac_data);
            h.finalize().into_bytes().to_vec()
        } else {
            let mut h = Hmac::<Sha256>::new_from_slice(&mac_key).unwrap();
            h.update(&mac_data);
            h.finalize().into_bytes().to_vec()
        };
        let mut body = padded.clone();
        let private_out = if encrypted {
            let len = body.len();
            cbc::Encryptor::<Aes256>::new_from_slices(&key, &iv)
                .unwrap()
                .encrypt_padded_mut::<NoPadding>(&mut body, len)
                .unwrap()
                .to_vec()
        } else {
            padded
        };
        let (pub_n, pub_b64) = b64_lines(public_blob);
        let (priv_n, priv_b64) = b64_lines(&private_out);
        let mut file = format!(
            "PuTTY-User-Key-File-{version}: {algorithm}\nEncryption: {encryption}\nComment: {comment}\nPublic-Lines: {pub_n}\n{pub_b64}\n"
        );
        if version >= 3 && encrypted {
            let spec = argon.unwrap();
            file.push_str(&format!(
                "Key-Derivation: {}\nArgon2-Memory: {}\nArgon2-Passes: {}\nArgon2-Parallelism: {}\nArgon2-Salt: {}\n",
                spec.flavour,
                spec.memory,
                spec.passes,
                spec.lanes,
                hex::encode(spec.salt)
            ));
        }
        file.push_str(&format!(
            "Private-Lines: {priv_n}\n{priv_b64}\nPrivate-MAC: {}\n",
            hex::encode(mac)
        ));
        file
    }

    fn ed25519_material(seed: &[u8; 32]) -> (Vec<u8>, Vec<u8>) {
        let signing = SigningKey::from_bytes(seed);
        let mut public = Vec::new();
        push_string(&mut public, b"ssh-ed25519");
        push_string(&mut public, signing.verifying_key().as_bytes());
        let mut private = Vec::new();
        push_string(&mut private, seed);
        (public, private)
    }

    fn rsa_material() -> (Vec<u8>, Vec<u8>) {
        // Throwaway 32-bit textbook key. p=61 q=53 n=3233 e=17 d=2753 iqmp=38.
        let mut public = Vec::new();
        push_string(&mut public, b"ssh-rsa");
        push_mpint(&mut public, &[17]);
        push_mpint(&mut public, &3233u16.to_be_bytes());
        let mut private = Vec::new();
        push_mpint(&mut private, &2753u16.to_be_bytes());
        push_mpint(&mut private, &[61]);
        push_mpint(&mut private, &[53]);
        push_mpint(&mut private, &[38]);
        (public, private)
    }

    fn ecdsa_material() -> (Vec<u8>, Vec<u8>) {
        let scalar = [0x11u8; 32];
        let key = russh_keys::ec::PrivateKey::new_from_secret_scalar(b"ecdsa-sha2-nistp256", &scalar).unwrap();
        let point = key.to_public_key().to_sec1_bytes();
        let mut public = Vec::new();
        push_string(&mut public, b"ecdsa-sha2-nistp256");
        push_string(&mut public, b"nistp256");
        push_string(&mut public, &point);
        let mut private = Vec::new();
        push_mpint(&mut private, &scalar);
        (public, private)
    }

    fn tiny_argon() -> Argon {
        Argon {
            flavour: "Argon2id",
            memory: 8,
            passes: 1,
            lanes: 1,
            salt: b"tracelyn-salt!!",
        }
    }

    fn assert_ed25519(bytes: &[u8], pass: Option<&str>, seed: &[u8; 32]) {
        match load_key_pair(bytes, pass).unwrap() {
            KeyPair::Ed25519(key) => assert_eq!(key.to_bytes(), *seed),
            _ => panic!("expected ed25519"),
        }
    }

    fn assert_rsa(bytes: &[u8], pass: Option<&str>) {
        match load_key_pair(bytes, pass).unwrap() {
            KeyPair::RSA { .. } => {}
            _ => panic!("expected rsa"),
        }
    }

    #[test]
    fn v2_aes_key_matches_sha1_spec() {
        let pass = b"secret";
        let d0 = Sha1::digest([0u32.to_be_bytes().as_slice(), pass.as_slice()].concat());
        let d1 = Sha1::digest([1u32.to_be_bytes().as_slice(), pass.as_slice()].concat());
        let mut expect = [0u8; 32];
        expect[..20].copy_from_slice(&d0);
        expect[20..].copy_from_slice(&d1[..12]);
        let (public, private) = ed25519_material(&[9u8; 32]);
        let file = seal(2, "ssh-ed25519", "t", &public, &private, Some("secret"), None);
        let parsed = parse(file.as_bytes()).unwrap();
        let (key, iv, _) = derive(&parsed, b"secret").unwrap();
        assert_eq!(key, expect);
        assert_eq!(iv, [0u8; 16]);
    }

    #[test]
    fn roundtrip_v2_v3_rsa_and_ed25519() {
        let seed = [7u8; 32];
        let (epub, epriv) = ed25519_material(&seed);
        let (rpub, rpriv) = rsa_material();
        let argon = tiny_argon();
        let cases = [
            (2u8, false, "ssh-ed25519"),
            (2, true, "ssh-ed25519"),
            (3, false, "ssh-ed25519"),
            (3, true, "ssh-ed25519"),
            (2, false, "ssh-rsa"),
            (2, true, "ssh-rsa"),
            (3, false, "ssh-rsa"),
            (3, true, "ssh-rsa"),
        ];
        for (version, encrypted, alg) in cases {
            let pass = encrypted.then_some("test-passphrase");
            let (public, private) = if alg == "ssh-rsa" {
                (rpub.clone(), rpriv.clone())
            } else {
                (epub.clone(), epriv.clone())
            };
            let file = seal(
                version,
                alg,
                "throwaway",
                &public,
                &private,
                pass,
                encrypted.then_some(&argon),
            );
            if alg == "ssh-rsa" {
                assert_rsa(file.as_bytes(), pass);
            } else {
                assert_ed25519(file.as_bytes(), pass, &seed);
            }
        }
    }

    #[test]
    fn argon2_i_and_d_roundtrip() {
        let seed = [3u8; 32];
        let (public, private) = ed25519_material(&seed);
        for flavour in ["Argon2i", "Argon2d"] {
            let argon = Argon {
                flavour,
                memory: 8,
                passes: 1,
                lanes: 1,
                salt: b"another-salt!!",
            };
            let file = seal(3, "ssh-ed25519", "k", &public, &private, Some("pw"), Some(&argon));
            assert_ed25519(file.as_bytes(), Some("pw"), &seed);
        }
    }

    #[test]
    fn ecdsa_v2_and_v3() {
        let (public, private) = ecdsa_material();
        let argon = tiny_argon();
        for (version, encrypted) in [(2u8, false), (3, true)] {
            let pass = encrypted.then_some("ec-pass");
            let file = seal(
                version,
                "ecdsa-sha2-nistp256",
                "ec",
                &public,
                &private,
                pass,
                encrypted.then_some(&argon),
            );
            match load_key_pair(file.as_bytes(), pass).unwrap() {
                KeyPair::EC { key } => {
                    assert_eq!(key.algorithm(), "ecdsa-sha2-nistp256");
                }
                _ => panic!("expected ecdsa"),
            }
        }
    }

    #[test]
    fn wrong_passphrase_corrupt_and_unsupported() {
        let seed = [4u8; 32];
        let (public, private) = ed25519_material(&seed);
        let argon = tiny_argon();
        let enc = seal(3, "ssh-ed25519", "k", &public, &private, Some("right"), Some(&argon));
        assert_eq!(
            load_key_pair(enc.as_bytes(), Some("wrong")).unwrap_err(),
            PpkError::WrongPassphrase
        );
        assert_eq!(load_key_pair(enc.as_bytes(), None).unwrap_err(), PpkError::NeedsPassphrase);
        let plain = seal(2, "ssh-ed25519", "k", &public, &private, None, None);
        let mut broken = plain.clone();
        let mac_at = broken.rfind("Private-MAC: ").unwrap() + "Private-MAC: ".len();
        let flip = if broken.as_bytes()[mac_at] == b'a' { "b" } else { "a" };
        broken.replace_range(mac_at..mac_at + 1, flip);
        assert_eq!(load_key_pair(broken.as_bytes(), None).unwrap_err(), PpkError::Corrupt);
        let dss = b"PuTTY-User-Key-File-2: ssh-dss\nEncryption: none\nComment: x\nPublic-Lines: 1\nAAAA\nPrivate-Lines: 1\nAAAA\nPrivate-MAC: 00\n";
        assert_eq!(load_key_pair(dss, None).unwrap_err(), PpkError::UnsupportedAlgorithm);
        let v1 = b"PuTTY-User-Key-File-1: ssh-rsa\nEncryption: none\nComment: x\n";
        assert_eq!(load_key_pair(v1, None).unwrap_err(), PpkError::UnsupportedVersion);
        assert!(load_key_pair(b"not a key", None).unwrap_err() == PpkError::Corrupt);
        let crlf = plain.replace('\n', "\r\n");
        assert_ed25519(crlf.as_bytes(), None, &seed);
    }
}

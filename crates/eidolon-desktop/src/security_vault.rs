//! Encrypted vault (AES-GCM + Argon2id) — Phase F optional surface.
//!
//! wraps: `aes-gcm` 0.10 — authenticated encryption
//! wraps: `argon2` 0.5 — Argon2id key derivation / password PHC hashes
//! wraps: `sha2` — salt compression helper only
//!
//! Enable with feature `desktop-security-vault`. Feature-off → fail-loud
//! [`codes::DESKTOP_SECURITY_UNAVAILABLE`] via [`crate::security_hooks`].

use crate::codes;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::password_hash::{PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Argon2, PasswordHash};
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Env override for vault file path (`EIDOLON_DESKTOP_VAULT_PATH`).
pub const VAULT_PATH_ENV: &str = "EIDOLON_DESKTOP_VAULT_PATH";

const MAGIC: &[u8] = b"EIDLV1\0";
const NONCE_LEN: usize = 12;
const SALT_LEN: usize = 16;

fn vault_unavailable(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::DESKTOP_SECURITY_UNAVAILABLE,
        format!("EncryptedVault::{method} unavailable — {detail}"),
    )
}

/// Resolve durable vault path (never `/tmp` by default).
pub fn default_vault_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var(VAULT_PATH_ENV) {
        let path = PathBuf::from(p);
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::Normal(s) if s == "tmp"))
            || path.starts_with("/tmp")
            || path.starts_with("/var/folders")
        {
            return Err(vault_unavailable(
                "path",
                format!("{VAULT_PATH_ENV} must not use ephemeral tmp paths"),
            ));
        }
        return Ok(path);
    }
    Ok(dirs_state_home()?.join("eidolon").join("vault.eidlv1"))
}

fn dirs_state_home() -> Result<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        return Ok(PathBuf::from(xdg));
    }
    let home = std::env::var("HOME")
        .map_err(|_| vault_unavailable("path", "HOME unset and XDG_STATE_HOME unset"))?;
    Ok(PathBuf::from(home).join(".local").join("state"))
}

fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| vault_unavailable("kdf", format!("Argon2id failed: {e}")))?;
    Ok(key)
}

fn fill_random(buf: &mut [u8]) -> Result<()> {
    OsRng
        .try_fill_bytes(buf)
        .map_err(|e| vault_unavailable("rng", e))
}

/// Encrypt `plaintext` under `passphrase` → magic|salt|nonce|ciphertext.
pub fn seal(passphrase: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
    if passphrase.is_empty() {
        return Err(vault_unavailable("seal", "passphrase must not be empty"));
    }
    let mut salt = [0u8; SALT_LEN];
    fill_random(&mut salt)?;
    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    fill_random(&mut nonce_bytes)?;
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|e| vault_unavailable("seal", format!("AES-GCM encrypt failed: {e}")))?;

    let mut out = Vec::with_capacity(MAGIC.len() + SALT_LEN + NONCE_LEN + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt vault blob with `passphrase`.
pub fn open(passphrase: &str, blob: &[u8]) -> Result<Vec<u8>> {
    if passphrase.is_empty() {
        return Err(vault_unavailable("open", "passphrase must not be empty"));
    }
    if blob.len() < MAGIC.len() + SALT_LEN + NONCE_LEN + 16 {
        return Err(vault_unavailable("open", "blob too short / corrupt"));
    }
    if &blob[..MAGIC.len()] != MAGIC {
        return Err(vault_unavailable("open", "bad magic (not EIDLV1 vault)"));
    }
    let salt = &blob[MAGIC.len()..MAGIC.len() + SALT_LEN];
    let nonce = &blob[MAGIC.len() + SALT_LEN..MAGIC.len() + SALT_LEN + NONCE_LEN];
    let ct = &blob[MAGIC.len() + SALT_LEN + NONCE_LEN..];
    let key = derive_key(passphrase, salt)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    cipher
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|_| {
            vault_unavailable("open", "AES-GCM decrypt failed (bad passphrase or corrupt)")
        })
}

/// Seal and write to `path` (creates parent dirs).
pub fn seal_to_path(passphrase: &str, plaintext: &[u8], path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            vault_unavailable("seal_to_path", format!("mkdir {}: {e}", parent.display()))
        })?;
    }
    let bytes = seal(passphrase, plaintext)?;
    fs::write(path, bytes)
        .map_err(|e| vault_unavailable("seal_to_path", format!("write {}: {e}", path.display())))
}

/// Read + open vault at `path`.
pub fn open_from_path(passphrase: &str, path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path).map_err(|e| {
        vault_unavailable("open_from_path", format!("read {}: {e}", path.display()))
    })?;
    open(passphrase, &bytes)
}

/// Hash a password for storage (PHC string).
pub fn hash_password(passphrase: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(passphrase.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| vault_unavailable("hash_password", e))
}

/// Verify a PHC password hash.
pub fn verify_password(passphrase: &str, phc: &str) -> Result<bool> {
    let parsed = PasswordHash::new(phc)
        .map_err(|e| vault_unavailable("verify_password", format!("bad PHC: {e}")))?;
    Ok(Argon2::default()
        .verify_password(passphrase.as_bytes(), &parsed)
        .is_ok())
}

/// Fingerprint vault blob (lowercase hex SHA-256) for integrity logs.
pub fn blob_fingerprint(blob: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(blob);
    h.finalize().iter().fold(String::new(), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Traces to: FR-EIDOLON-001
    #[test]
    fn seal_open_roundtrip() {
        let blob = seal("test-passphrase-ok", b"secret-payload").unwrap();
        let plain = open("test-passphrase-ok", &blob).unwrap();
        assert_eq!(plain, b"secret-payload");
        assert!(open("wrong", &blob).is_err());
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn password_hash_verify() {
        let phc = hash_password("hunter2").unwrap();
        assert!(verify_password("hunter2", &phc).unwrap());
        assert!(!verify_password("nope", &phc).unwrap());
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn empty_passphrase_fails_loud() {
        let err = seal("", b"x").unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_SECURITY_UNAVAILABLE)
        );
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn tmp_vault_path_rejected() {
        std::env::set_var(VAULT_PATH_ENV, "/tmp/eidolon-vault");
        let err = default_vault_path().unwrap_err();
        std::env::remove_var(VAULT_PATH_ENV);
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_SECURITY_UNAVAILABLE)
        );
    }
}

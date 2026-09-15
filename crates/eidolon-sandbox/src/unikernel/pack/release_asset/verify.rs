//! SHA-256 checksum verification for release assets.

use std::path::Path;

use eidolon_core::error::PhenoError;
use eidolon_core::Result;

use super::release_unavailable;
use crate::codes;

/// Hex-encode SHA-256 of file contents (streaming).
pub fn sha256_file(path: &Path) -> Result<String> {
    #[cfg(feature = "sandbox-rootfs-pack")]
    {
        use std::fs;
        use std::io::Read;

        use sha2::{Digest, Sha256};
        let mut file = fs::File::open(path).map_err(|e| super::pack_io(path, e))?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = file.read(&mut buf).map_err(|e| super::pack_io(path, e))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hex_encode_digest(&hasher.finalize()))
    }
    #[cfg(not(feature = "sandbox-rootfs-pack"))]
    {
        let _ = path;
        Err(PhenoError::unsupported_platform(
            codes::SANDBOX_ROOTFS_PACK_STUB,
            "sha256_file / release asset checksums require feature `sandbox-rootfs-pack`",
        ))
    }
}

#[cfg(feature = "sandbox-rootfs-pack")]
pub(crate) fn hex_encode_digest(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

/// Verify `path` exists and matches `expected_sha256` (lowercase hex).
pub fn verify_image(path: &Path, expected_sha256: &str) -> Result<()> {
    if !path.is_file() {
        return Err(release_unavailable(
            "verify_image",
            format!("Ext4 image missing: {}", path.display()),
        ));
    }
    let expected = expected_sha256.trim().to_ascii_lowercase();
    if expected.is_empty() || expected.len() != 64 {
        return Err(release_unavailable(
            "verify_image",
            "expected SHA-256 must be 64 lowercase hex chars (pin unpublished?)",
        ));
    }
    let actual = sha256_file(path).map_err(|e| {
        release_unavailable("verify_image", format!("hash {}: {e}", path.display()))
    })?;
    if actual != expected {
        return Err(release_unavailable(
            "verify_image",
            format!(
                "SHA-256 mismatch for {} — expected {expected}, got {actual} \
                 (delete cache and re-run eidolon-fetch-rootfs)",
                path.display()
            ),
        ));
    }
    Ok(())
}

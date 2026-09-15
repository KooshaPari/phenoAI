//! Hash and integrity chain verification for audit entries.

use eidolon_core::error::PhenoError;
use eidolon_core::Result;

use super::AuditEntry;
use crate::codes;

/// Hash payload for integrity chaining.
pub(crate) fn hash_hex(bytes: &[u8]) -> String {
    #[cfg(feature = "sandbox-audit")]
    {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(bytes);
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }
    #[cfg(not(feature = "sandbox-audit"))]
    {
        // Feature-off: deterministic non-crypto fingerprint so chain linkage
        // tests still work without claiming cryptographic integrity.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        format!("fnv1a64:{h:016x}")
    }
}

pub(crate) fn entry_content_hash(entry: &AuditEntry) -> Result<String> {
    // Hash without chain_hash field to avoid self-reference.
    let mut for_hash = entry.clone();
    for_hash.chain_hash = None;
    let serialized = serde_json::to_string(&for_hash).map_err(|e| {
        PhenoError::Internal(format!(
            "[{}] audit serialize for hash failed: {e}",
            codes::SANDBOX_AUDIT_IO
        ))
    })?;
    Ok(hash_hex(serialized.as_bytes()))
}

pub(crate) fn chain_hash(previous: Option<&str>, entry_hash: &str) -> String {
    let payload = format!("{}:{}", previous.unwrap_or("GENESIS"), entry_hash);
    hash_hex(payload.as_bytes())
}

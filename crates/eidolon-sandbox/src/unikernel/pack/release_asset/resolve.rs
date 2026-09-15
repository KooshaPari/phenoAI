//! Release-asset pin/resolve logic (env → checkout constants → effective pin).

use std::path::PathBuf;

use eidolon_core::Result;

use super::{
    release_unavailable, ROOTFS_ASSET_SHA256_ENV, ROOTFS_ASSET_URL_ENV, ROOTFS_RELEASE_FILENAME,
    ROOTFS_RELEASE_PUBLISHED, ROOTFS_RELEASE_SHA256, ROOTFS_RELEASE_URL,
};

/// Active Ext4 release pin (env override or checkout constants).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootfsReleasePin {
    pub url: String,
    pub sha256: String,
    pub filename: String,
    /// `true` when resolved from [`ROOTFS_ASSET_URL_ENV`] + SHA env.
    pub from_env: bool,
}

/// Derive a cache filename from an asset URL (last path segment), else checkout default.
pub fn filename_from_asset_url(url: &str) -> String {
    url.trim()
        .rsplit('/')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.contains('.') && !s.contains('?'))
        .map(|s| s.to_string())
        .unwrap_or_else(|| ROOTFS_RELEASE_FILENAME.to_string())
}

/// Parse optional env pin ([`ROOTFS_ASSET_URL_ENV`] + [`ROOTFS_ASSET_SHA256_ENV`]).
///
/// Both must be set together; one without the other → fail-loud
/// ([`codes::SANDBOX_ROOTFS_RELEASE_UNAVAILABLE`]). Neither → `Ok(None)`.
pub fn env_asset_pin() -> Result<Option<RootfsReleasePin>> {
    let url = std::env::var(ROOTFS_ASSET_URL_ENV).ok();
    let sha = std::env::var(ROOTFS_ASSET_SHA256_ENV).ok();
    let url_t = url.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let sha_t = sha.as_deref().map(str::trim).filter(|s| !s.is_empty());
    match (url_t, sha_t) {
        (Some(u), Some(s)) => {
            if s.len() != 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(release_unavailable(
                    "env_asset_pin",
                    format!(
                        "{ROOTFS_ASSET_SHA256_ENV} must be 64 ASCII hex chars (got len {})",
                        s.len()
                    ),
                ));
            }
            Ok(Some(RootfsReleasePin {
                url: u.to_string(),
                sha256: s.to_ascii_lowercase(),
                filename: filename_from_asset_url(u),
                from_env: true,
            }))
        }
        (Some(_), None) | (None, Some(_)) => Err(release_unavailable(
            "env_asset_pin",
            format!(
                "incomplete env pin — set both {ROOTFS_ASSET_URL_ENV} and \
                 {ROOTFS_ASSET_SHA256_ENV} (or neither); fail-loud — no silent fetch"
            ),
        )),
        (None, None) => Ok(None),
    }
}

/// Checkout constants pin when [`ROOTFS_RELEASE_PUBLISHED`] and URL/SHA filled.
pub fn checkout_asset_pin() -> Option<RootfsReleasePin> {
    if !ROOTFS_RELEASE_PUBLISHED
        || ROOTFS_RELEASE_URL.trim().is_empty()
        || ROOTFS_RELEASE_SHA256.trim().is_empty()
    {
        return None;
    }
    Some(RootfsReleasePin {
        url: ROOTFS_RELEASE_URL.trim().to_string(),
        sha256: ROOTFS_RELEASE_SHA256.trim().to_ascii_lowercase(),
        filename: ROOTFS_RELEASE_FILENAME.to_string(),
        from_env: false,
    })
}

/// Effective pin: env (`EIDOLON_ROOTFS_ASSET_*`) wins, else checkout constants.
///
/// Incomplete env pair → [`Err`]. No pin available → `Ok(None)` (caller fail-louds).
pub fn effective_release_pin() -> Result<Option<RootfsReleasePin>> {
    if let Some(pin) = env_asset_pin()? {
        return Ok(Some(pin));
    }
    Ok(checkout_asset_pin())
}

/// Checkout pin directory (`…/assets/canned-rootfs`).
pub fn checkout_assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("canned-rootfs")
}

/// Path to the checkout pin JSON.
pub fn checkout_release_manifest_path() -> PathBuf {
    checkout_assets_dir().join("release-manifest.json")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::codes;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn pin_published_constants() {
        assert!(ROOTFS_RELEASE_PUBLISHED);
        assert!(!ROOTFS_RELEASE_URL.is_empty());
        assert_eq!(ROOTFS_RELEASE_SHA256.len(), 64);
        assert!(checkout_release_manifest_path().is_file());
        let pin = checkout_asset_pin().expect("published pin");
        assert_eq!(pin.sha256, ROOTFS_RELEASE_SHA256);
        assert_eq!(pin.url, ROOTFS_RELEASE_URL);
        assert!(!pin.from_env);
    }

    #[test]
    fn filename_from_url_uses_last_segment() {
        assert_eq!(
            filename_from_asset_url(
                "https://github.com/KooshaPari/Eidolon/releases/download/rootfs-v0.1.0/\
                 eidolon-canned-rootfs-0.1.0-x86_64.ext4.img"
            ),
            "eidolon-canned-rootfs-0.1.0-x86_64.ext4.img"
        );
        assert_eq!(
            filename_from_asset_url("https://example.invalid/no-segment"),
            ROOTFS_RELEASE_FILENAME
        );
    }

    #[test]
    fn env_asset_pin_incomplete_fails_loud() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(ROOTFS_ASSET_URL_ENV);
        std::env::remove_var(ROOTFS_ASSET_SHA256_ENV);
        assert!(env_asset_pin().unwrap().is_none());

        std::env::set_var(ROOTFS_ASSET_URL_ENV, "https://example.invalid/a.img");
        let err = env_asset_pin().unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_ROOTFS_RELEASE_UNAVAILABLE)
        );
        assert!(err.to_string().contains("incomplete"));
        std::env::remove_var(ROOTFS_ASSET_URL_ENV);

        std::env::set_var(ROOTFS_ASSET_SHA256_ENV, "abcd");
        let err = env_asset_pin().unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_ROOTFS_RELEASE_UNAVAILABLE)
        );
        std::env::remove_var(ROOTFS_ASSET_SHA256_ENV);
    }

    #[test]
    fn env_asset_pin_parses_url_and_sha() {
        let _g = ENV_LOCK.lock().unwrap();
        let sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        std::env::set_var(
            ROOTFS_ASSET_URL_ENV,
            "https://example.invalid/eidolon-canned-rootfs-0.1.0-x86_64.ext4.img",
        );
        std::env::set_var(ROOTFS_ASSET_SHA256_ENV, sha);
        let pin = env_asset_pin().unwrap().expect("pin");
        assert!(pin.from_env);
        assert_eq!(pin.sha256, sha);
        assert_eq!(pin.filename, "eidolon-canned-rootfs-0.1.0-x86_64.ext4.img");
        let effective = effective_release_pin().unwrap().expect("effective");
        assert_eq!(effective, pin);
        std::env::remove_var(ROOTFS_ASSET_URL_ENV);
        std::env::remove_var(ROOTFS_ASSET_SHA256_ENV);
    }

    #[test]
    fn checkout_manifest_parses_and_marks_published() {
        let path = checkout_release_manifest_path();
        use super::super::{
            ReleaseAssetManifest, RELEASE_MANIFEST_SCHEMA_VERSION, ROOTFS_RELEASE_VERSION,
        };
        let m = ReleaseAssetManifest::read_from(&path).expect("pin json");
        assert_eq!(m.schema_version, RELEASE_MANIFEST_SCHEMA_VERSION);
        assert!(m.published);
        assert_eq!(m.sha256.as_deref(), Some(ROOTFS_RELEASE_SHA256));
        assert_eq!(m.version, ROOTFS_RELEASE_VERSION);
    }
}

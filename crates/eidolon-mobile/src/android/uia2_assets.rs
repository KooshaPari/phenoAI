//! Official Appium UiAutomator2 APK resolution, SHA-256 verify, and fetch.
//!
//! Binary APKs (~17 MB server) are **not** git-vendored. Pinned digests live in
//! [`manifest.json`](../../../assets/uia2/manifest.json). Resolution order:
//!
//! 1. Env — [`crate::cli::UIA2_APK_ENV`] / [`crate::cli::UIA2_TEST_APK_ENV`]
//! 2. Checkout assets — `crates/eidolon-mobile/assets/uia2/`
//! 3. Durable cache — [`UIA2_CACHE_ENV`] or `~/.cache/eidolon/uia2/<version>/`
//! 4. Fail loud — [`codes::MOBILE_UIA2_UNAVAILABLE`]
//!
//! [`ensure`] downloads into the durable cache on miss (SHA-256 verified).
//! Cache roots under `/tmp` are rejected. Do not unarchive kmobile.

use crate::cli::{
    env_uia2_apk, env_uia2_test_apk, UIA2_APK_ENV, UIA2_CACHE_ENV, UIA2_TEST_APK_ENV,
};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Pinned Appium UiAutomator2 server release tag.
pub const UIA2_PINNED_VERSION: &str = "v10.3.2";

/// Upstream release page (attribution / upgrades).
pub const UIA2_RELEASE_SOURCE: &str =
    "https://github.com/appium/appium-uiautomator2-server/releases/tag/v10.3.2";

/// Server APK filename for the pinned release.
pub const SERVER_APK_FILENAME: &str = "appium-uiautomator2-server-v10.3.2.apk";

/// Instrumentation / test APK filename for the pinned release.
pub const TEST_APK_FILENAME: &str = "appium-uiautomator2-server-debug-androidTest.apk";

/// SHA-256 of the pinned server APK (lowercase hex).
pub const SERVER_APK_SHA256: &str =
    "8463a42f7701bf29d07571089e189ff10178c5f6dab2b65ae8cce7018d44fbd8";

/// SHA-256 of the pinned test APK (lowercase hex).
pub const TEST_APK_SHA256: &str =
    "3eb8ed926f98a0b29248f0271c8d4df2fd34977298f79645e685338d971630cc";

/// Official download URL for the pinned server APK.
pub const SERVER_APK_URL: &str = "https://github.com/appium/appium-uiautomator2-server/releases/download/v10.3.2/appium-uiautomator2-server-v10.3.2.apk";

/// Official download URL for the pinned test APK.
pub const TEST_APK_URL: &str = "https://github.com/appium/appium-uiautomator2-server/releases/download/v10.3.2/appium-uiautomator2-server-debug-androidTest.apk";

fn uia2_unavailable(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::MOBILE_UIA2_UNAVAILABLE,
        format!(
            "Uia2Assets::{method} unavailable — {detail}; resolution: \
             {UIA2_APK_ENV}+{UIA2_TEST_APK_ENV} → assets/uia2 → \
             {UIA2_CACHE_ENV}|~/.cache/eidolon/uia2/{UIA2_PINNED_VERSION}; \
             run `cargo run -p eidolon-mobile --features mobile-uia2 \
             --bin eidolon-fetch-uia2` or call Uia2ApkPaths::ensure(); \
             see crates/eidolon-mobile/assets/uia2/README.md \
             (do not unarchive kmobile)"
        ),
    )
}

/// Checkout-relative assets directory (`…/assets/uia2`).
pub fn checkout_assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join("uia2")
}

/// Default durable cache directory for the pinned version.
///
/// Order: [`UIA2_CACHE_ENV`] → `$HOME/.cache/eidolon/uia2/<version>`.
/// Fails loud if the chosen root is under `/tmp`.
pub fn durable_cache_dir() -> Result<PathBuf> {
    let root = if let Ok(override_root) = std::env::var(UIA2_CACHE_ENV) {
        let trimmed = override_root.trim();
        if trimmed.is_empty() {
            return Err(uia2_unavailable(
                "durable_cache_dir",
                format!("{UIA2_CACHE_ENV} is set but empty"),
            ));
        }
        PathBuf::from(trimmed)
    } else {
        let home = std::env::var_os("HOME").ok_or_else(|| {
            uia2_unavailable(
                "durable_cache_dir",
                "HOME unset and EIDOLON_UIA2_CACHE unset — cannot place durable cache",
            )
        })?;
        PathBuf::from(home)
            .join(".cache")
            .join("eidolon")
            .join("uia2")
            .join(UIA2_PINNED_VERSION)
    };
    guard_against_tmp(&root)?;
    Ok(root)
}

/// Reject cache / output roots under `/tmp` (agent-infra durability).
pub fn guard_against_tmp(path: &Path) -> Result<()> {
    let raw = path.to_string_lossy();
    let lower = raw.to_ascii_lowercase();
    if lower.starts_with("/tmp")
        || lower.starts_with("/private/tmp")
        || lower.contains("/tmp/")
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::Normal(s) if s == "tmp"))
    {
        return Err(uia2_unavailable(
            "guard_against_tmp",
            format!(
                "refusing path under /tmp ({raw}) — set {UIA2_CACHE_ENV} to a \
                 durable directory (e.g. ~/.cache/eidolon/uia2/{UIA2_PINNED_VERSION})"
            ),
        ));
    }
    Ok(())
}

/// Hex-encode SHA-256 of file contents.
pub fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|e| {
        uia2_unavailable(
            "sha256_file",
            format!("read {}: {e}", path.display()),
        )
    })?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Verify `path` exists and matches `expected_sha256` (lowercase hex).
pub fn verify_apk(path: &Path, expected_sha256: &str) -> Result<()> {
    if !path.is_file() {
        return Err(uia2_unavailable(
            "verify_apk",
            format!("APK missing: {}", path.display()),
        ));
    }
    let actual = sha256_file(path)?;
    if actual != expected_sha256 {
        return Err(uia2_unavailable(
            "verify_apk",
            format!(
                "SHA-256 mismatch for {} — expected {expected_sha256}, got {actual} \
                 (delete the file and re-run eidolon-fetch-uia2)",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn pair_from_dir(dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let server = dir.join(SERVER_APK_FILENAME);
    let test = dir.join(TEST_APK_FILENAME);
    if server.is_file() && test.is_file() {
        Some((server, test))
    } else {
        None
    }
}

fn verified_pair(server: PathBuf, test: PathBuf) -> Result<super::uia2_server::Uia2ApkPaths> {
    verify_apk(&server, SERVER_APK_SHA256)?;
    verify_apk(&test, TEST_APK_SHA256)?;
    Ok(super::uia2_server::Uia2ApkPaths::new(server, test))
}

/// Resolve APKs without network I/O.
///
/// Env overrides skip SHA-256 checks (operator-supplied BYO). Checkout assets
/// and durable cache are always hash-verified against the pinned release.
pub fn resolve() -> Result<super::uia2_server::Uia2ApkPaths> {
    resolve_with_roots(Some(&checkout_assets_dir()), durable_cache_dir().ok().as_deref())
}

/// Resolve with injectable roots (hermetic tests).
pub fn resolve_with_roots(
    assets_dir: Option<&Path>,
    cache_dir: Option<&Path>,
) -> Result<super::uia2_server::Uia2ApkPaths> {
    let env_server = env_uia2_apk();
    let env_test = env_uia2_test_apk();
    match (env_server, env_test) {
        (Some(server), Some(test)) => {
            return Ok(super::uia2_server::Uia2ApkPaths::new(server, test));
        }
        (Some(_), None) => {
            return Err(uia2_unavailable(
                "resolve",
                format!("{UIA2_APK_ENV} set but {UIA2_TEST_APK_ENV} unset or not a file"),
            ));
        }
        (None, Some(_)) => {
            return Err(uia2_unavailable(
                "resolve",
                format!("{UIA2_TEST_APK_ENV} set but {UIA2_APK_ENV} unset or not a file"),
            ));
        }
        (None, None) => {}
    }

    if let Some(dir) = assets_dir {
        if let Some((server, test)) = pair_from_dir(dir) {
            return verified_pair(server, test);
        }
    }

    if let Some(dir) = cache_dir {
        guard_against_tmp(dir)?;
        if let Some((server, test)) = pair_from_dir(dir) {
            return verified_pair(server, test);
        }
    }

    Err(uia2_unavailable(
        "resolve",
        format!(
            "no UiAutomator2 APKs found for {UIA2_PINNED_VERSION} — set env, \
             place APKs under assets/uia2, or fetch into durable cache"
        ),
    ))
}

/// Resolve, or download pinned APKs into the durable cache (verified).
///
/// Requires feature `mobile-uia2` (ureq + TLS). Env overrides still win and
/// skip download.
#[cfg(feature = "mobile-uia2")]
pub fn ensure() -> Result<super::uia2_server::Uia2ApkPaths> {
    match resolve() {
        Ok(paths) => Ok(paths),
        Err(_) => {
            let cache = durable_cache_dir()?;
            fetch_into(&cache)?;
            resolve_with_roots(Some(&checkout_assets_dir()), Some(&cache))
        }
    }
}

/// Download both pinned APKs into `dest_dir`, verifying SHA-256.
#[cfg(feature = "mobile-uia2")]
pub fn fetch_into(dest_dir: &Path) -> Result<()> {
    guard_against_tmp(dest_dir)?;
    fs::create_dir_all(dest_dir).map_err(|e| {
        uia2_unavailable(
            "fetch_into",
            format!("mkdir {}: {e}", dest_dir.display()),
        )
    })?;
    download_verified(
        SERVER_APK_URL,
        &dest_dir.join(SERVER_APK_FILENAME),
        SERVER_APK_SHA256,
    )?;
    download_verified(
        TEST_APK_URL,
        &dest_dir.join(TEST_APK_FILENAME),
        TEST_APK_SHA256,
    )?;
    Ok(())
}

#[cfg(feature = "mobile-uia2")]
fn download_verified(url: &str, dest: &Path, expected_sha256: &str) -> Result<()> {
    use std::process::Command;

    if dest.is_file() {
        match verify_apk(dest, expected_sha256) {
            Ok(()) => return Ok(()),
            Err(_) => {
                let _ = fs::remove_file(dest);
            }
        }
    }

    let tmp = dest.with_extension("apk.partial");
    let _ = fs::remove_file(&tmp);

    // System curl for HTTPS GitHub release assets. Keep ureq TLS-free so the
    // localhost UIA2 HTTP client + hermetic mocks stay stable (no rustls).
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--retry",
            "3",
            "--retry-delay",
            "1",
            "-o",
            tmp.to_str().ok_or_else(|| {
                uia2_unavailable("download_verified", "APK path is not valid UTF-8")
            })?,
            url,
        ])
        .output()
        .map_err(|e| {
            uia2_unavailable(
                "download_verified",
                format!("curl spawn failed ({e}) — install curl or set {UIA2_APK_ENV}"),
            )
        })?;
    if !output.status.success() {
        let _ = fs::remove_file(&tmp);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(uia2_unavailable(
            "download_verified",
            format!("curl GET {url} failed: {}", stderr.trim()),
        ));
    }

    verify_apk(&tmp, expected_sha256).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e
    })?;
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        uia2_unavailable(
            "download_verified",
            format!("rename {} → {}: {e}", tmp.display(), dest.display()),
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn pinned_constants_match_manifest_filenames() {
        assert!(SERVER_APK_FILENAME.contains(UIA2_PINNED_VERSION.trim_start_matches('v'))
            || SERVER_APK_FILENAME.contains("v10.3.2"));
        assert_eq!(SERVER_APK_SHA256.len(), 64);
        assert_eq!(TEST_APK_SHA256.len(), 64);
        assert!(SERVER_APK_URL.contains(UIA2_PINNED_VERSION));
    }

    #[test]
    fn guard_rejects_tmp() {
        let err = guard_against_tmp(Path::new("/tmp/eidolon-uia2")).unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::MOBILE_UIA2_UNAVAILABLE));
        let err = guard_against_tmp(Path::new("/private/tmp/x")).unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::MOBILE_UIA2_UNAVAILABLE));
    }

    #[test]
    fn guard_allows_durable_cache_shape() {
        assert!(guard_against_tmp(Path::new("/Users/me/.cache/eidolon/uia2/v10.3.2")).is_ok());
    }

    #[test]
    fn resolve_with_empty_roots_fails_loud_when_env_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        if env_uia2_apk().is_some() || env_uia2_test_apk().is_some() {
            return;
        }
        let err = resolve_with_roots(None, None).unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::MOBILE_UIA2_UNAVAILABLE));
        assert!(err.to_string().contains("eidolon-fetch-uia2") || err.to_string().contains("APK"));
    }

    #[test]
    fn verify_apk_rejects_missing() {
        let err = verify_apk(Path::new("/nonexistent/uia2-server.apk"), SERVER_APK_SHA256)
            .unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::MOBILE_UIA2_UNAVAILABLE));
    }

    #[test]
    fn sha256_hex_known_empty() {
        // SHA-256 of empty input
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn resolve_prefers_env_over_roots() {
        let _g = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "eidolon-uia2-env-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let server = dir.join("server.apk");
        let test = dir.join("test.apk");
        fs::write(&server, b"server-bytes").unwrap();
        fs::write(&test, b"test-bytes").unwrap();
        std::env::set_var(UIA2_APK_ENV, &server);
        std::env::set_var(UIA2_TEST_APK_ENV, &test);
        let paths = resolve_with_roots(None, None).expect("env pair");
        assert_eq!(paths.server, server);
        assert_eq!(paths.test, test);
        std::env::remove_var(UIA2_APK_ENV);
        std::env::remove_var(UIA2_TEST_APK_ENV);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_verifies_pair_in_assets_dir() {
        let _g = ENV_LOCK.lock().unwrap();
        if env_uia2_apk().is_some() {
            return;
        }
        // Use real cache if already fetched (happy path); else fail-loud hermetic.
        let cache = PathBuf::from(std::env::var_os("HOME").unwrap())
            .join(".cache")
            .join("eidolon")
            .join("uia2")
            .join(UIA2_PINNED_VERSION);
        if pair_from_dir(&cache).is_some() {
            let paths = resolve_with_roots(None, Some(&cache)).expect("cached APKs");
            assert!(paths.server.is_file());
            assert!(paths.test.is_file());
        } else {
            let err = resolve_with_roots(None, Some(&cache)).unwrap_err();
            assert_eq!(err.unsupported_code(), Some(codes::MOBILE_UIA2_UNAVAILABLE));
        }
    }
}

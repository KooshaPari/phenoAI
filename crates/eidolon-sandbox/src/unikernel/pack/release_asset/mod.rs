//! Versioned GitHub release Ext4 rootfs assets + pin/download client.

pub mod fetch;
mod resolve;
mod staging;
pub mod verify;

use std::fs;
use std::path::{Path, PathBuf};

use eidolon_core::error::PhenoError;
use eidolon_core::Result;
#[cfg(feature = "sandbox-rootfs-pack")]
pub use fetch::fetch_into;
pub use fetch::{
    ensure as ensure_rootfs_release, resolve as resolve_rootfs_release,
    resolve_with_roots as resolve_rootfs_release_with_roots,
};
pub use resolve::{
    checkout_asset_pin, checkout_assets_dir, checkout_release_manifest_path, effective_release_pin,
    env_asset_pin, filename_from_asset_url, RootfsReleasePin,
};
pub use staging::{
    build_and_stage_release_rootfs, gh_upload_hints, release_canned_request, stage_release_asset,
    ReleaseAssetManifest, ReleaseAssetRequest, ReleaseAssetResult,
};
pub use verify::{
    sha256_file as rootfs_release_sha256_file, verify_image as verify_rootfs_release,
};

use super::canned::{
    build_canned_rootfs, guard_against_tmp as canned_guard_against_tmp, resolve_canned_out_dir,
    CannedMode, CannedRootfsRequest, CannedRootfsResult, CANNED_ROOTFS_IMG, CANNED_ROOTFS_OUT_ENV,
};
use super::{pack_io, DiskImageBackend};
use crate::codes;

pub(crate) fn release_unavailable(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_ROOTFS_RELEASE_UNAVAILABLE,
        format!(
            "RootfsRelease::{method} unavailable — {detail}; resolution: \
             {ROOTFS_IMG_ENV} → {ROOTFS_ASSET_URL_ENV}+{ROOTFS_ASSET_SHA256_ENV} → \
             {ROOTFS_RELEASE_CACHE_ENV}|~/.cache/eidolon/rootfs-release/\
             {ROOTFS_RELEASE_VERSION}; see docs/guides/gh-ext4-release.md (fail-loud)"
        ),
    )
}

pub const ROOTFS_IMG_ENV: &str = "EIDOLON_ROOTFS_IMG";
pub const ROOTFS_ASSET_URL_ENV: &str = "EIDOLON_ROOTFS_ASSET_URL";
pub const ROOTFS_ASSET_SHA256_ENV: &str = "EIDOLON_ROOTFS_ASSET_SHA256";
pub const ROOTFS_RELEASE_CACHE_ENV: &str = "EIDOLON_ROOTFS_RELEASE_CACHE";
pub const ROOTFS_RELEASE_OUT_ENV: &str = "EIDOLON_ROOTFS_RELEASE_OUT";
pub const DEFAULT_RELEASE_OUT_REL: &str = "target/rootfs-release";
pub const RELEASE_ASSET_MANIFEST: &str = "eidolon-rootfs-release-manifest.json";
pub const RELEASE_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_RELEASE_ARCH: &str = "x86_64";
pub const ROOTFS_RELEASE_PUBLISHED: bool = true;
pub const ROOTFS_RELEASE_VERSION: &str = "0.1.0";
pub const ROOTFS_RELEASE_FILENAME: &str = "eidolon-canned-rootfs-0.1.0-x86_64.ext4.img";
pub const ROOTFS_RELEASE_URL: &str = "https://github.com/KooshaPari/Eidolon/releases/download/rootfs-v0.1.0/eidolon-canned-rootfs-0.1.0-x86_64.ext4.img";
pub const ROOTFS_RELEASE_SHA256: &str =
    "edee5005e4b206667faff33decbada12d57fb825d59cdb659b838f4857368b03";
pub const ROOTFS_RELEASE_SOURCE: &str = "https://github.com/KooshaPari/Eidolon/releases";

pub fn versioned_image_filename(version: &str, arch: &str) -> String {
    let ver = version.trim().trim_start_matches('v');
    format!("eidolon-canned-rootfs-{ver}-{arch}.ext4.img")
}
pub fn sha256_sidecar_filename(image_filename: &str) -> String {
    format!("{image_filename}.sha256")
}
pub fn guard_against_tmp(path: &Path) -> Result<()> {
    canned_guard_against_tmp(path).map_err(|e| match e {
        PhenoError::BadRequest(msg) => release_unavailable("guard_against_tmp", msg),
        other => other,
    })
}
pub fn resolve_release_out_dir(explicit: Option<&Path>) -> Result<PathBuf> {
    let dir = if let Some(p) = explicit {
        p.to_path_buf()
    } else if let Ok(override_path) = std::env::var(ROOTFS_RELEASE_OUT_ENV) {
        let trimmed = override_path.trim();
        if trimmed.is_empty() {
            return Err(PhenoError::BadRequest(format!(
                "{ROOTFS_RELEASE_OUT_ENV} is set but empty"
            )));
        }
        PathBuf::from(trimmed)
    } else {
        default_release_out_dir()
    };
    guard_against_tmp(&dir)?;
    fs::create_dir_all(&dir).map_err(|e| pack_io(&dir, e))?;
    Ok(dir)
}
fn default_release_out_dir() -> PathBuf {
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let crate_dir = PathBuf::from(manifest);
        if let Some(ws) = crate_dir.parent().and_then(|p| p.parent()) {
            return ws.join(DEFAULT_RELEASE_OUT_REL);
        }
    }
    PathBuf::from(DEFAULT_RELEASE_OUT_REL)
}
pub fn durable_cache_dir() -> Result<PathBuf> {
    let root = if let Ok(override_root) = std::env::var(ROOTFS_RELEASE_CACHE_ENV) {
        let trimmed = override_root.trim();
        if trimmed.is_empty() {
            return Err(release_unavailable(
                "durable_cache_dir",
                format!("{ROOTFS_RELEASE_CACHE_ENV} is set but empty"),
            ));
        }
        PathBuf::from(trimmed)
    } else {
        let home = std::env::var_os("HOME")
            .ok_or_else(|| release_unavailable("durable_cache_dir", "HOME unset"))?;
        PathBuf::from(home)
            .join(".cache")
            .join("eidolon")
            .join("rootfs-release")
            .join(ROOTFS_RELEASE_VERSION)
    };
    guard_against_tmp(&root)?;
    Ok(root)
}
pub fn canned_out_env_name() -> &'static str {
    CANNED_ROOTFS_OUT_ENV
}
pub fn resolve_canned_out_for_release(explicit: Option<&Path>) -> Result<PathBuf> {
    resolve_canned_out_dir(explicit)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::codes;

    #[test]
    fn versioned_filename_strips_v_prefix() {
        assert_eq!(
            versioned_image_filename("v0.1.0", "x86_64"),
            "eidolon-canned-rootfs-0.1.0-x86_64.ext4.img"
        );
        assert_eq!(
            versioned_image_filename("0.1.0", "aarch64"),
            "eidolon-canned-rootfs-0.1.0-aarch64.ext4.img"
        );
    }
    #[test]
    fn guard_rejects_tmp() {
        let err = guard_against_tmp(Path::new("/tmp/eidolon-rootfs-release")).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_ROOTFS_RELEASE_UNAVAILABLE)
        );
    }
}

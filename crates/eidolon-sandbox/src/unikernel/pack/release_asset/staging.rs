//! Release-asset staging logic (versioned Ext4 + SHA-256 sidecar + manifest).

use std::fs;
use std::path::{Path, PathBuf};

use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use serde::{Deserialize, Serialize};

use super::{
    resolve_release_out_dir, sha256_sidecar_filename, verify, versioned_image_filename, CannedMode,
    CannedRootfsRequest, CannedRootfsResult, DiskImageBackend,
};
use crate::codes;

/// Inputs for staging a versioned release asset from an existing Ext4 image.
#[derive(Debug, Clone)]
pub struct ReleaseAssetRequest {
    /// Existing Ext4 disk (e.g. canned `rootfs.img`).
    pub source_img: PathBuf,
    /// Release version / tag (e.g. `0.1.0` or `v0.1.0`).
    pub version: String,
    /// Guest arch label in the filename (default [`super::DEFAULT_RELEASE_ARCH`]).
    pub arch: String,
    /// Staging directory for upload artifacts.
    pub out_dir: Option<PathBuf>,
    /// When true, compute SHA-256 (requires `sandbox-rootfs-pack`).
    pub compute_checksum: bool,
}

impl ReleaseAssetRequest {
    pub fn new(source_img: impl Into<PathBuf>, version: impl Into<String>) -> Self {
        Self {
            source_img: source_img.into(),
            version: version.into(),
            arch: super::DEFAULT_RELEASE_ARCH.into(),
            out_dir: None,
            compute_checksum: true,
        }
    }

    pub fn with_arch(mut self, arch: impl Into<String>) -> Self {
        self.arch = arch.into();
        self
    }

    pub fn with_out_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.out_dir = Some(dir.into());
        self
    }

    pub fn with_checksum(mut self, compute: bool) -> Self {
        self.compute_checksum = compute;
        self
    }
}

/// JSON manifest written beside the versioned image (and used as the pin template).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAssetManifest {
    pub schema_version: u32,
    /// Whether this pin points at a real published GH release asset.
    pub published: bool,
    pub version: String,
    pub arch: String,
    pub filename: String,
    pub sha256_sidecar: String,
    /// Absolute or staging-relative path to the versioned image after staging.
    pub image_path: PathBuf,
    pub sha256: Option<String>,
    pub bytes: Option<u64>,
    /// Suggested `gh release upload` URL once published (may be empty).
    pub url: Option<String>,
    pub honesty: String,
}

impl ReleaseAssetManifest {
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| {
            PhenoError::Internal(format!(
                "{}: release asset manifest serialize failed: {e}",
                codes::SANDBOX_ROOTFS_PACK_IO
            ))
        })
    }

    pub fn from_json(s: &str) -> Result<Self> {
        serde_json::from_str(s).map_err(|e| {
            PhenoError::Internal(format!(
                "{}: release asset manifest parse failed: {e}",
                codes::SANDBOX_ROOTFS_PACK_IO
            ))
        })
    }

    pub fn write_to(&self, path: &Path) -> Result<()> {
        let json = self.to_json()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| super::pack_io(path, e))?;
        }
        fs::write(path, json.as_bytes()).map_err(|e| super::pack_io(path, e))
    }

    pub fn read_from(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path).map_err(|e| super::pack_io(path, e))?;
        Self::from_json(&s)
    }
}

/// Result of staging a versioned release asset.
#[derive(Debug, Clone)]
pub struct ReleaseAssetResult {
    pub out_dir: PathBuf,
    pub image_path: PathBuf,
    pub sha256_path: Option<PathBuf>,
    pub manifest_path: PathBuf,
    pub manifest: ReleaseAssetManifest,
    /// When staged after a canned build, the underlying canned result.
    pub canned: Option<CannedRootfsResult>,
}

/// Stage a versioned Ext4 image + SHA-256 sidecar + JSON for `gh release upload`.
///
/// Does **not** call GitHub — operator runs `gh release upload` (see guide).
/// Checksums require feature `sandbox-rootfs-pack` when `compute_checksum`.
pub fn stage_release_asset(req: &ReleaseAssetRequest) -> Result<ReleaseAssetResult> {
    if !req.source_img.is_file() {
        return Err(PhenoError::BadRequest(format!(
            "release asset source_img must be an existing file; got {}",
            req.source_img.display()
        )));
    }
    let version = req.version.trim();
    if version.is_empty() {
        return Err(PhenoError::BadRequest(
            "release asset version must be non-empty (e.g. 0.1.0 or v0.1.0)".into(),
        ));
    }
    let arch = req.arch.trim();
    if arch.is_empty() {
        return Err(PhenoError::BadRequest(
            "release asset arch must be non-empty (default x86_64)".into(),
        ));
    }

    let out_dir = resolve_release_out_dir(req.out_dir.as_deref())?;
    let filename = versioned_image_filename(version, arch);
    let image_path = out_dir.join(&filename);
    if image_path != req.source_img {
        if image_path.exists() {
            fs::remove_file(&image_path).map_err(|e| super::pack_io(&image_path, e))?;
        }
        fs::copy(&req.source_img, &image_path).map_err(|e| super::pack_io(&image_path, e))?;
    }

    let bytes = fs::metadata(&image_path)
        .map_err(|e| super::pack_io(&image_path, e))?
        .len();

    let (sha256, sha256_path) = if req.compute_checksum {
        let hex = verify::sha256_file(&image_path)?;
        let side = out_dir.join(sha256_sidecar_filename(&filename));
        let body = format!("{hex}  {filename}\n");
        fs::write(&side, body.as_bytes()).map_err(|e| super::pack_io(&side, e))?;
        (Some(hex), Some(side))
    } else {
        (None, None)
    };

    let manifest = ReleaseAssetManifest {
        schema_version: super::RELEASE_MANIFEST_SCHEMA_VERSION,
        published: false,
        version: version.trim_start_matches('v').to_string(),
        arch: arch.to_string(),
        filename: filename.clone(),
        sha256_sidecar: sha256_sidecar_filename(&filename),
        image_path: image_path.clone(),
        sha256,
        bytes: Some(bytes),
        url: None,
        honesty: "Staged locally — not yet a published GH release asset. Upload with \
                  `gh release upload` then update assets/canned-rootfs/release-manifest.json \
                  + release_asset pin constants (docs/guides/gh-ext4-release.md)."
            .into(),
    };
    let manifest_path = out_dir.join(super::RELEASE_ASSET_MANIFEST);
    manifest.write_to(&manifest_path)?;

    Ok(ReleaseAssetResult {
        out_dir,
        image_path,
        sha256_path,
        manifest_path,
        manifest,
        canned: None,
    })
}

/// Build canned Ext4 (live gates) then stage a versioned release asset.
///
/// Requires `sandbox-rootfs-pack` + `ROOTFS_PACK_INTEGRATION=1` + tools + agent.
pub fn build_and_stage_release_rootfs(
    canned: &CannedRootfsRequest,
    version: &str,
    arch: Option<&str>,
    release_out: Option<&Path>,
) -> Result<ReleaseAssetResult> {
    let mut req = canned.clone();
    req.mode = match req.mode {
        CannedMode::TreeOnly => CannedMode::PackExt4,
        other => other,
    };
    req.compute_checksum = true;
    let canned_result = super::build_canned_rootfs(&req)?;
    let img = canned_result.rootfs_img.as_ref().ok_or_else(|| {
        PhenoError::Internal(format!(
            "{}: canned build did not produce {}",
            codes::SANDBOX_ROOTFS_PACK_IO,
            super::CANNED_ROOTFS_IMG
        ))
    })?;
    let stage = ReleaseAssetRequest::new(img, version)
        .with_arch(arch.unwrap_or(super::DEFAULT_RELEASE_ARCH))
        .with_checksum(true);
    let stage = if let Some(out) = release_out {
        stage.with_out_dir(out)
    } else {
        stage
    };
    let mut result = stage_release_asset(&stage)?;
    result.canned = Some(canned_result);
    Ok(result)
}

/// Convenience: PackExt4 canned request defaults for release builds.
pub fn release_canned_request(
    agent: Option<PathBuf>,
    canned_out: Option<PathBuf>,
) -> CannedRootfsRequest {
    let mut req = CannedRootfsRequest::new()
        .with_mode(CannedMode::PackExt4)
        .with_systemd_unit(true)
        .with_checksum(true)
        .with_disk_backend(DiskImageBackend::MkfsExt4);
    if let Some(a) = agent {
        req = req.with_agent_bin(a);
    }
    if let Some(o) = canned_out {
        req = req.with_out_dir(o);
    }
    req
}

/// Print suggested `gh release` commands for a staged asset (operator copy-paste).
pub fn gh_upload_hints(result: &ReleaseAssetResult, tag: &str) -> String {
    let img = result.image_path.display();
    let side = result
        .sha256_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<missing-sha256>".into());
    let man = result.manifest_path.display();
    format!(
        "# Create (or reuse) a GitHub release, then upload:\n\
         gh release create {tag} --title \"Eidolon canned rootfs {tag}\" --notes \"Ext4 disk with eidolon-vsock-agent\" --draft\n\
         gh release upload {tag} \\\n\
           {img} \\\n\
           {side} \\\n\
           {man}\n\
         # After upload: fill assets/canned-rootfs/release-manifest.json + pin constants,\n\
         # set ROOTFS_RELEASE_PUBLISHED=true — or export EIDOLON_ROOTFS_ASSET_URL+\n\
         # EIDOLON_ROOTFS_ASSET_SHA256 without flipping checkout defaults\n\
         # (docs/guides/gh-ext4-release.md).\n"
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Write;

    use super::*;
    use crate::codes;

    pub(crate) fn temp_dir(name: &str) -> PathBuf {
        let root = if let Ok(td) = std::env::var("CARGO_TARGET_DIR") {
            PathBuf::from(td)
        } else if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
            PathBuf::from(m)
                .parent()
                .and_then(|p| p.parent())
                .unwrap_or(Path::new("."))
                .join("target")
        } else {
            PathBuf::from("target")
        };
        let path = root.join("rootfs-release-test").join(format!(
            "{}-{}-{}",
            name,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("mkdir");
        path
    }

    pub(crate) fn fake_img(dir: &Path) -> PathBuf {
        let p = dir.join("rootfs.img");
        let mut f = fs::File::create(&p).unwrap();
        writeln!(f, "fake-ext4-for-release-staging").unwrap();
        p
    }

    #[cfg(feature = "sandbox-rootfs-pack")]
    #[test]
    fn stage_writes_versioned_img_sha_and_manifest() {
        let out = temp_dir("stage");
        let host = temp_dir("host-img");
        let src = fake_img(&host);
        let result = stage_release_asset(
            &ReleaseAssetRequest::new(&src, "v0.0.1-test")
                .with_out_dir(&out)
                .with_checksum(true),
        )
        .expect("stage");
        assert!(result.image_path.is_file());
        assert!(result
            .image_path
            .ends_with("eidolon-canned-rootfs-0.0.1-test-x86_64.ext4.img"));
        let side = result.sha256_path.as_ref().expect("sha sidecar");
        assert!(side.is_file());
        let side_body = fs::read_to_string(side).unwrap();
        assert!(side_body.contains(&result.manifest.sha256.clone().unwrap()));
        assert!(result.manifest_path.is_file());
        assert!(!result.manifest.published);
        assert_eq!(result.manifest.version, "0.0.1-test");
        let hex = result.manifest.sha256.clone().unwrap();
        assert_eq!(hex.len(), 64);
        verify::verify_image(&result.image_path, &hex).unwrap();
        let hints = gh_upload_hints(&result, "v0.0.1-test");
        assert!(hints.contains("gh release upload"));
        let _ = fs::remove_dir_all(&out);
        let _ = fs::remove_dir_all(&host);
    }
}

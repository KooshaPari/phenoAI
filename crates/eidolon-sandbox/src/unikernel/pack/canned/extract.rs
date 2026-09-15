//! Canned rootfs tree preparation, agent resolution, and build logic.

use std::fs;
use std::path::{Path, PathBuf};

use eidolon_core::error::PhenoError;
use eidolon_core::Result;

use super::super::bake_agent::{
    resolve_vsock_agent_bin, stage_vsock_agent_into_tree, BakeAgentRequest, BakeAgentResult,
    AGENT_PATH_ENV, DEFAULT_GUEST_AGENT_REL, DEFAULT_GUEST_UNIT_REL,
};
use super::super::{
    bake_agent_then_pack, pack_io, PackMethod, PackRequest, PackageManifest, CANNED_ROOTFS_IMG,
    CANNED_TREE_DIR, CANNED_TREE_MANIFEST,
};
use super::{
    guard_against_tmp, CannedMode, CannedRootfsRequest, CannedRootfsResult, AGENT_CACHE_ENV,
    AGENT_SHA256_ENV, AGENT_URL_ENV,
};
use crate::codes;
use crate::unikernel::RootfsFormat;

/// Seed a minimal stub rootfs tree at `dest` (hermetic; no real Linux userland).
///
/// Creates `usr/local/bin`, `etc/systemd/system`, and a `README.eidolon` honesty
/// marker. Does **not** install the agent — call [`stage_vsock_agent_into_tree`].
pub fn seed_minimal_rootfs_tree(dest: &Path) -> Result<()> {
    guard_against_tmp(dest)?;
    fs::create_dir_all(dest.join("usr/local/bin")).map_err(|e| pack_io(dest, e))?;
    fs::create_dir_all(dest.join("etc/systemd/system")).map_err(|e| pack_io(dest, e))?;
    let readme = dest.join("README.eidolon");
    if !readme.is_file() {
        fs::write(
            &readme,
            b"Eidolon canned stub rootfs tree - not a full Linux userland.\n\
              Agent is staged at usr/local/bin/eidolon-vsock-agent by the canned pipeline.\n\
              Real Ext4 release disks are built on Linux (mkfs/virt-make-fs + musl agent).\n",
        )
        .map_err(|e| pack_io(&readme, e))?;
    }
    Ok(())
}

/// Copy the checkout minimal-tree fixture into `dest` (or seed if fixture absent).
pub fn materialize_base_tree(dest: &Path) -> Result<()> {
    let fixture = super::checkout_minimal_tree_fixture();
    if fixture.is_dir() {
        copy_dir_recursive(&fixture, dest)?;
        seed_minimal_rootfs_tree(dest)?;
        return Ok(());
    }
    seed_minimal_rootfs_tree(dest)
}

pub(crate) fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest).map_err(|e| pack_io(dest, e))?;
    for entry in fs::read_dir(src).map_err(|e| pack_io(src, e))? {
        let entry = entry.map_err(|e| pack_io(src, e))?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|e| pack_io(parent, e))?;
            }
            fs::copy(&from, &to).map_err(|e| pack_io(&to, e))?;
        }
    }
    Ok(())
}

/// Resolve agent binary for canned bake.
///
/// Order: explicit → [`AGENT_PATH_ENV`] / cargo discovery → optional pin fetch
/// when `allow_fetch` and [`AGENT_URL_ENV`]+[`AGENT_SHA256_ENV`] are set.
/// Missing → [`codes::SANDBOX_VSOCK_AGENT_MISSING`].
pub fn ensure_vsock_agent_bin(explicit: Option<&Path>, allow_fetch: bool) -> Result<PathBuf> {
    match resolve_vsock_agent_bin(explicit) {
        Ok(p) => return Ok(p),
        Err(e) => {
            if !allow_fetch {
                return Err(e);
            }
            if e.unsupported_code() != Some(codes::SANDBOX_VSOCK_AGENT_MISSING) {
                return Err(e);
            }
        }
    }

    let url = std::env::var(AGENT_URL_ENV).ok();
    let sha = std::env::var(AGENT_SHA256_ENV).ok();
    match (url.as_deref().map(str::trim), sha.as_deref().map(str::trim)) {
        (Some(u), Some(s)) if !u.is_empty() && !s.is_empty() => {
            #[cfg(feature = "sandbox-rootfs-pack")]
            {
                return fetch_pinned_agent(u, s);
            }
            #[cfg(not(feature = "sandbox-rootfs-pack"))]
            {
                let _ = (u, s);
                return Err(PhenoError::unsupported_platform(
                    codes::SANDBOX_VSOCK_AGENT_MISSING,
                    format!(
                        "eidolon-vsock-agent pin fetch requires feature `sandbox-rootfs-pack` \
                         (SHA-256) — set {AGENT_PATH_ENV} to a built Linux musl binary, or \
                         enable the feature and set {AGENT_URL_ENV}+{AGENT_SHA256_ENV} \
                         (docs/guides/canned-rootfs.md)"
                    ),
                ));
            }
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(PhenoError::unsupported_platform(
                codes::SANDBOX_VSOCK_AGENT_MISSING,
                format!(
                    "agent pin incomplete — set both {AGENT_URL_ENV} and {AGENT_SHA256_ENV}, \
                     or pass an explicit binary / {AGENT_PATH_ENV} (docs/guides/canned-rootfs.md)"
                ),
            ));
        }
        _ => {}
    }

    Err(PhenoError::unsupported_platform(
        codes::SANDBOX_VSOCK_AGENT_MISSING,
        format!(
            "eidolon-vsock-agent missing for canned rootfs — set {AGENT_PATH_ENV} to a \
             Linux musl binary (built on Linux CI), pass CannedRootfsRequest::agent_bin, \
             or set {AGENT_URL_ENV}+{AGENT_SHA256_ENV} with allow_agent_fetch; hermetic \
             tests use a stub binary. Fail-loud — no silent empty bake \
             (docs/guides/canned-rootfs.md)"
        ),
    ))
}

#[cfg(feature = "sandbox-rootfs-pack")]
fn agent_cache_dir() -> Result<PathBuf> {
    let root = if let Ok(override_root) = std::env::var(AGENT_CACHE_ENV) {
        let trimmed = override_root.trim();
        if trimmed.is_empty() {
            return Err(PhenoError::BadRequest(format!(
                "{AGENT_CACHE_ENV} is set but empty"
            )));
        }
        PathBuf::from(trimmed)
    } else {
        let home = std::env::var_os("HOME").ok_or_else(|| {
            PhenoError::unsupported_platform(
                codes::SANDBOX_VSOCK_AGENT_MISSING,
                format!(
                    "HOME unset and {AGENT_CACHE_ENV} unset — cannot place durable agent cache"
                ),
            )
        })?;
        PathBuf::from(home)
            .join(".cache")
            .join("eidolon")
            .join("vsock-agent")
    };
    guard_against_tmp(&root)?;
    fs::create_dir_all(&root).map_err(|e| pack_io(&root, e))?;
    Ok(root)
}

#[cfg(feature = "sandbox-rootfs-pack")]
fn fetch_pinned_agent(url: &str, expected_sha256: &str) -> Result<PathBuf> {
    use std::process::Command;

    let cache = agent_cache_dir()?;
    let dest = cache.join("eidolon-vsock-agent");
    if dest.is_file() {
        if verify_sha256_file(&dest, expected_sha256).is_ok() {
            return Ok(dest);
        }
        let _ = fs::remove_file(&dest);
    }

    let tmp = dest.with_extension("partial");
    let _ = fs::remove_file(&tmp);

    let output = Command::new("curl")
        .args(["-fsSL", "--retry", "3", "-o"])
        .arg(&tmp)
        .arg(url)
        .output()
        .map_err(|e| {
            PhenoError::unsupported_platform(
                codes::SANDBOX_VSOCK_AGENT_MISSING,
                format!(
                    "curl failed to fetch pinned eidolon-vsock-agent from {url}: {e} \
                     — set {AGENT_PATH_ENV} to a local Linux binary instead"
                ),
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_file(&tmp);
        return Err(PhenoError::unsupported_platform(
            codes::SANDBOX_VSOCK_AGENT_MISSING,
            format!(
                "curl exit {} fetching {url}: {stderr} — fail-loud pin miss \
                 (docs/guides/canned-rootfs.md)",
                output.status
            ),
        ));
    }

    verify_sha256_file(&tmp, expected_sha256)?;
    fs::rename(&tmp, &dest).map_err(|e| pack_io(&dest, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dest)
            .map_err(|e| pack_io(&dest, e))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms).map_err(|e| pack_io(&dest, e))?;
    }
    Ok(dest)
}

#[cfg(feature = "sandbox-rootfs-pack")]
fn verify_sha256_file(path: &Path, expected: &str) -> Result<()> {
    use std::io::Read;

    use sha2::{Digest, Sha256};

    let mut file = fs::File::open(path).map_err(|e| pack_io(path, e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).map_err(|e| pack_io(path, e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let actual = hex_encode_digest(&digest);
    if actual != expected.to_ascii_lowercase() {
        return Err(PhenoError::unsupported_platform(
            codes::SANDBOX_VSOCK_AGENT_MISSING,
            format!(
                "SHA-256 mismatch for {} — expected {expected}, got {actual} \
                 (delete cache and re-fetch; docs/guides/canned-rootfs.md)",
                path.display()
            ),
        ));
    }
    Ok(())
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

fn bake_req_for(req: &CannedRootfsRequest, agent: &Path) -> BakeAgentRequest {
    BakeAgentRequest::new()
        .with_agent_bin(agent)
        .with_systemd_unit(req.bake_systemd_unit)
        .with_link(req.link_artifacts)
}

pub(crate) fn build_tree_only(
    req: &CannedRootfsRequest,
    out_dir: &Path,
    agent: &Path,
) -> Result<CannedRootfsResult> {
    let tree = out_dir.join(CANNED_TREE_DIR);
    if tree.exists() {
        if tree.is_dir() {
            fs::remove_dir_all(&tree).map_err(|e| pack_io(&tree, e))?;
        } else {
            fs::remove_file(&tree).map_err(|e| pack_io(&tree, e))?;
        }
    }
    if let Some(base) = &req.base_tree {
        if !base.is_dir() {
            return Err(PhenoError::BadRequest(format!(
                "canned base_tree must be a directory; got {}",
                base.display()
            )));
        }
        copy_dir_recursive(base, &tree)?;
        seed_minimal_rootfs_tree(&tree)?;
    } else {
        materialize_base_tree(&tree)?;
    }

    let bake = stage_vsock_agent_into_tree(&tree, &bake_req_for(req, agent))?;
    let agent_guest = tree.join(DEFAULT_GUEST_AGENT_REL);
    if !agent_guest.is_file() {
        return Err(PhenoError::Internal(format!(
            "{}: canned bake did not produce {}",
            codes::SANDBOX_ROOTFS_PACK_IO,
            agent_guest.display()
        )));
    }

    let manifest = PackageManifest {
        schema_version: super::super::MANIFEST_SCHEMA_VERSION,
        rootfs: tree.clone(),
        rootfs_format: "raw".into(),
        kernel: None,
        rootfs_checksum: None,
        kernel_checksum: None,
        pack_method: PackMethod::Hermetic,
        staging_dir: Some(out_dir.to_path_buf()),
    };
    let manifest_path = out_dir.join(CANNED_TREE_MANIFEST);
    manifest.write_to(&manifest_path)?;

    Ok(CannedRootfsResult {
        mode: CannedMode::TreeOnly,
        out_dir: out_dir.to_path_buf(),
        rootfs_tree: tree,
        rootfs_img: None,
        bake,
        manifest,
    })
}

pub(crate) fn build_pack_ext4(
    req: &CannedRootfsRequest,
    out_dir: &Path,
    agent: &Path,
) -> Result<CannedRootfsResult> {
    let tree_result = build_tree_only(req, out_dir, agent)?;
    let staging = out_dir.join("pack-staging");
    fs::create_dir_all(&staging).map_err(|e| pack_io(&staging, e))?;

    let method = req.disk_backend.as_pack_method();
    let mut pack_req = PackRequest::hermetic(&tree_result.rootfs_tree, RootfsFormat::Ext4)
        .with_method(method)
        .with_staging(&staging, req.link_artifacts)
        .with_disk_backend(req.disk_backend)
        .with_bake_vsock_agent(true)
        .with_vsock_agent_bin(agent)
        .with_bake_systemd_unit(req.bake_systemd_unit)
        .with_checksum(req.compute_checksum);
    if let Some(sz) = req.image_size_bytes {
        pack_req = pack_req.with_image_size(sz);
    }

    let manifest = bake_agent_then_pack(&pack_req)?;
    let img_dest = out_dir.join(CANNED_ROOTFS_IMG);
    if manifest.rootfs != img_dest {
        if img_dest.exists() {
            fs::remove_file(&img_dest).map_err(|e| pack_io(&img_dest, e))?;
        }
        fs::copy(&manifest.rootfs, &img_dest).map_err(|e| pack_io(&img_dest, e))?;
    }
    let mut final_manifest = manifest;
    final_manifest.rootfs = img_dest.clone();
    final_manifest.staging_dir = Some(out_dir.to_path_buf());
    final_manifest.write_to(&out_dir.join(super::super::MANIFEST_FILENAME))?;

    Ok(CannedRootfsResult {
        mode: CannedMode::PackExt4,
        out_dir: out_dir.to_path_buf(),
        rootfs_tree: tree_result.rootfs_tree,
        rootfs_img: Some(img_dest),
        bake: tree_result.bake,
        manifest: final_manifest,
    })
}

pub(crate) fn build_docker_to_ext4(
    req: &CannedRootfsRequest,
    out_dir: &Path,
    agent: &Path,
) -> Result<CannedRootfsResult> {
    let docker_ref = req.docker_ref.as_deref().ok_or_else(|| {
        PhenoError::BadRequest(
            "CannedMode::DockerToExt4 requires docker_ref (image/container) \
             (docs/guides/canned-rootfs.md)"
                .into(),
        )
    })?;
    let staging = out_dir.join("pack-staging");
    fs::create_dir_all(&staging).map_err(|e| pack_io(&staging, e))?;

    let mut pack_req = PackRequest::hermetic(docker_ref, RootfsFormat::Ext4)
        .with_method(PackMethod::DockerToExt4)
        .with_staging(&staging, req.link_artifacts)
        .with_disk_backend(req.disk_backend)
        .with_bake_vsock_agent(true)
        .with_vsock_agent_bin(agent)
        .with_bake_systemd_unit(req.bake_systemd_unit)
        .with_checksum(req.compute_checksum);
    if let Some(sz) = req.image_size_bytes {
        pack_req = pack_req.with_image_size(sz);
    }

    let manifest = bake_agent_then_pack(&pack_req)?;
    let img_dest = out_dir.join(CANNED_ROOTFS_IMG);
    if manifest.rootfs != img_dest {
        if img_dest.exists() {
            fs::remove_file(&img_dest).map_err(|e| pack_io(&img_dest, e))?;
        }
        fs::copy(&manifest.rootfs, &img_dest).map_err(|e| pack_io(&img_dest, e))?;
    }
    let tree = staging.join("rootfs-tree");
    let bake = BakeAgentResult {
        agent_host_source: agent.to_path_buf(),
        agent_guest_path: tree.join(DEFAULT_GUEST_AGENT_REL),
        unit_guest_path: if req.bake_systemd_unit {
            Some(tree.join(DEFAULT_GUEST_UNIT_REL))
        } else {
            None
        },
    };
    let mut final_manifest = manifest;
    final_manifest.rootfs = img_dest.clone();
    final_manifest.staging_dir = Some(out_dir.to_path_buf());
    final_manifest.write_to(&out_dir.join(super::super::MANIFEST_FILENAME))?;

    Ok(CannedRootfsResult {
        mode: CannedMode::DockerToExt4,
        out_dir: out_dir.to_path_buf(),
        rootfs_tree: tree,
        rootfs_img: Some(img_dest),
        bake,
        manifest: final_manifest,
    })
}

//! Live pack strategies and compose pipelines.

use eidolon_core::error::PhenoError;
use eidolon_core::Result;

#[cfg(feature = "sandbox-rootfs-pack")]
use super::bake_agent::BakeAgentResult;
use super::hermetic::HermeticPackBuilder;
use super::pack_manifest::{PackMethod, PackRequest, PackageManifest, PACK_INTEGRATION_ENV};
use crate::codes;
#[cfg(feature = "sandbox-rootfs-pack")]
use crate::unikernel::RootfsFormat;

/// `true` when [`PACK_INTEGRATION_ENV`] is `"1"`.
pub fn pack_integration_enabled() -> bool {
    std::env::var(PACK_INTEGRATION_ENV).ok().as_deref() == Some("1")
}

/// Stage `eidolon-vsock-agent` into the rootfs tree, then run [`live_pack`].
///
/// Forces bake-on (even if `req.bake_vsock_agent` is false). Accepts
/// [`PackMethod::MkfsExt4`], [`PackMethod::VirtMakeFs`], or
/// [`PackMethod::DockerToExt4`]. For tree-only staging without packing, call
/// [`stage_vsock_agent_into_tree`] directly. Missing agent →
/// [`codes::SANDBOX_VSOCK_AGENT_MISSING`].
pub fn bake_agent_then_pack(req: &PackRequest) -> Result<PackageManifest> {
    let mut bake_req = req.clone();
    bake_req.bake_vsock_agent = true;
    bake_req.validate()?;

    match bake_req.pack_method {
        PackMethod::MkfsExt4 | PackMethod::VirtMakeFs | PackMethod::DockerToExt4 => {
            live_pack(&bake_req)
        }
        PackMethod::Hermetic | PackMethod::DockerExport => Err(PhenoError::BadRequest(
            "bake_agent_then_pack requires PackMethod::MkfsExt4, VirtMakeFs, or DockerToExt4 \
             (stage-only: unikernel::pack::stage_vsock_agent_into_tree; \
             docs/guides/vsock-guest-agent.md)"
                .into(),
        )),
    }
}

/// Apply bake into an existing rootfs tree when [`PackRequest::bake_vsock_agent`].
#[cfg(feature = "sandbox-rootfs-pack")]
pub(crate) fn maybe_bake_into_tree(
    tree: &std::path::Path,
    req: &PackRequest,
) -> Result<Option<BakeAgentResult>> {
    if !req.bake_vsock_agent {
        return Ok(None);
    }
    let bake = req.bake_agent_request();
    Ok(Some(stage_vsock_agent_into_tree(tree, &bake)?))
}

/// Live image packaging behind env + feature `sandbox-rootfs-pack`.
///
/// Hermetic requests are delegated to [`HermeticPackBuilder`]. Live methods
/// probe tools and fail loud when missing, when integration env is off, or
/// when the host command fails ([`codes::SANDBOX_ROOTFS_PACK_IO`]).
///
/// [`PackMethod::DockerToExt4`] composes docker export → disk image via
/// [`compose_docker_to_ext4`] (dual-tool probe).
pub fn live_pack(req: &PackRequest) -> Result<PackageManifest> {
    req.validate()?;
    if req.pack_method == PackMethod::Hermetic {
        return HermeticPackBuilder::new().build(req);
    }

    #[cfg(not(feature = "sandbox-rootfs-pack"))]
    {
        let _ = req;
        return Err(PhenoError::unsupported_platform(
            codes::SANDBOX_ROOTFS_PACK_STUB,
            format!(
                "live rootfs pack method={} requires feature `sandbox-rootfs-pack` \
                 and {}=1 (hermetic PackMethod::Hermetic works without the feature; \
                 docs/EXTRACTION_PLAN.md; do not unarchive KDesktopVirt routinely)",
                req.pack_method.as_str(),
                PACK_INTEGRATION_ENV
            ),
        ));
    }

    #[cfg(feature = "sandbox-rootfs-pack")]
    {
        if !pack_integration_enabled() {
            return Err(PhenoError::unsupported_platform(
                codes::SANDBOX_ROOTFS_PACK_STUB,
                format!(
                    "live rootfs pack method={} gated — set {}=1 to invoke host \
                     tools (default is hermetic-only; docs/EXTRACTION_PLAN.md)",
                    req.pack_method.as_str(),
                    PACK_INTEGRATION_ENV
                ),
            ));
        }

        if req.pack_method == PackMethod::DockerToExt4 {
            return compose_docker_to_ext4(req);
        }

        let tool = req
            .pack_method
            .required_tool()
            .expect("single-tool live methods declare a tool");
        let bin = tools::resolve_pack_tool(req.pack_method).ok_or_else(|| {
            PhenoError::unsupported_platform(
                codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING,
                format!(
                    "rootfs pack tool `{tool}` not found (PATH / EIDOLON_MKFS / \
                     EIDOLON_VIRT_MAKE_FS / EIDOLON_DOCKER); fail-loud — no silent \
                     fallback (docs/EXTRACTION_PLAN.md)"
                ),
            )
        })?;

        super::live::run_live_pack(&bin, req)
    }
}

/// Compose `docker export` → rootfs-tree → Ext4 disk (`mkfs.ext4` or
/// `virt-make-fs`) for Firecracker [`super::super::LaunchPlan`].
///
/// Same gates as [`live_pack`]: feature `sandbox-rootfs-pack` +
/// [`PACK_INTEGRATION_ENV`]=`1`. Fail-loud when docker **or** the disk
/// backend tool is missing — never labels a tarball as Ext4.
///
/// For an **existing** rootfs-tree directory (no docker), use
/// [`pack_rootfs_tree_to_ext4`] instead.
#[cfg(feature = "sandbox-rootfs-pack")]
pub fn compose_docker_to_ext4(req: &PackRequest) -> Result<PackageManifest> {
    req.validate()?;
    if !pack_integration_enabled() {
        return Err(PhenoError::unsupported_platform(
            codes::SANDBOX_ROOTFS_PACK_STUB,
            format!(
                "compose_docker_to_ext4 gated — set {}=1 (docs/reference/rootfs-pack.md)",
                PACK_INTEGRATION_ENV
            ),
        ));
    }
    let tools_pair = tools::resolve_docker_to_ext4_tools(req.disk_backend).ok_or_else(|| {
        let disk = req
            .disk_backend
            .as_pack_method()
            .required_tool()
            .unwrap_or("mkfs.ext4");
        PhenoError::unsupported_platform(
            codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING,
            format!(
                "docker_to_ext4 requires both `docker` and `{disk}` \
                 (PATH / EIDOLON_DOCKER / EIDOLON_MKFS / EIDOLON_VIRT_MAKE_FS); \
                 fail-loud — refuse Ext4 claim without a disk image tool \
                 (docs/reference/rootfs-pack.md)"
            ),
        )
    })?;
    super::live::run_docker_to_ext4(&tools_pair.docker, &tools_pair.disk, req)
}

/// Pack an existing rootfs-tree directory into an Ext4 disk image via
/// [`DiskImageBackend`] (mkfs or virt-make-fs). No docker required.
#[cfg(feature = "sandbox-rootfs-pack")]
pub fn pack_rootfs_tree_to_ext4(req: &PackRequest) -> Result<PackageManifest> {
    req.validate()?;
    if !req.rootfs_source.is_dir() {
        return Err(PhenoError::BadRequest(format!(
            "pack_rootfs_tree_to_ext4 requires rootfs_source to be a directory tree; got {}",
            req.rootfs_source.display()
        )));
    }
    let method = req.disk_backend.as_pack_method();
    let mut tree_req = req.clone();
    tree_req.pack_method = method;
    tree_req.rootfs_format = RootfsFormat::Ext4;
    live_pack(&tree_req)
}

/// Feature-off stubs so callers can name the compose APIs without `cfg`.
#[cfg(not(feature = "sandbox-rootfs-pack"))]
pub fn compose_docker_to_ext4(req: &PackRequest) -> Result<PackageManifest> {
    let _ = req;
    Err(PhenoError::unsupported_platform(
        codes::SANDBOX_ROOTFS_PACK_STUB,
        format!(
            "compose_docker_to_ext4 requires feature `sandbox-rootfs-pack` and {}=1 \
             (docs/reference/rootfs-pack.md)",
            PACK_INTEGRATION_ENV
        ),
    ))
}

#[cfg(not(feature = "sandbox-rootfs-pack"))]
pub fn pack_rootfs_tree_to_ext4(req: &PackRequest) -> Result<PackageManifest> {
    let _ = req;
    Err(PhenoError::unsupported_platform(
        codes::SANDBOX_ROOTFS_PACK_STUB,
        format!(
            "pack_rootfs_tree_to_ext4 requires feature `sandbox-rootfs-pack` and {}=1 \
             (docs/reference/rootfs-pack.md)",
            PACK_INTEGRATION_ENV
        ),
    ))
}

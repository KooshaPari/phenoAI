//! Rootfs / guest image packaging (hermetic + env-gated live).
//!
//! Always-on: [`PackageManifest`], [`PackRequest`], tool [`tools`] probes,
//! and [`HermeticPackBuilder`] (validate inputs, optional stage copy/link,
//! write manifest JSON). No production image is invented — hermetic sources
//! must already exist.
//!
//! Always-on: [`bake_agent`] stages `eidolon-vsock-agent` (+ optional systemd
//! unit) into a rootfs tree before pack — fail-loud
//! [`codes::SANDBOX_VSOCK_AGENT_MISSING`] when bake is requested and the
//! binary is absent (`EIDOLON_VSOCK_AGENT` preferred; cargo discovery for tests).
//!
//! Always-on: [`canned`] builds a durable canned rootfs tree (stub fixture +
//! baked agent) under [`canned::CANNED_ROOTFS_OUT_ENV`] /
//! `target/canned-rootfs/`. Live Ext4 (`CannedMode::PackExt4` /
//! `DockerToExt4`) wraps [`bake_agent_then_pack`] behind
//! `sandbox-rootfs-pack` + [`PACK_INTEGRATION_ENV`].
//!
//! Always-on: [`release_asset`] stages versioned Ext4 + SHA-256 sidecars for
//! `gh release upload`, plus a UIA2-shaped pin/download client. Default pin
//! may be unpublished — pipeline + client shipped; one-shot publish is
//! manual (billing-safe). See `docs/guides/gh-ext4-release.md`.
//!
//! Feature `sandbox-rootfs-pack`: SHA-256 checksums + live pack pipelines
//! (`mkfs.ext4` / `virt-make-fs` / `docker export` /
//! [`PackMethod::DockerToExt4`] compose) when [`PACK_INTEGRATION_ENV`] is
//! `"1"`. Missing tools → fail-loud
//! [`codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING`]. Feature-off / env-off live
//! paths → [`codes::SANDBOX_ROOTFS_PACK_STUB`]. Command failures →
//! [`codes::SANDBOX_ROOTFS_PACK_IO`].
//!
//! **Format honesty:** [`PackMethod::DockerExport`] always yields a tarball
//! labeled [`RootfsFormat::Raw`] — never `ext4`. Firecracker-bootable disks
//! require [`PackMethod::DockerToExt4`] (export → tree → mkfs/virt-make-fs)
//! or mkfs/virt-make-fs against an existing rootfs-tree.
//!
//! Builders produce paths [`super::RootfsConfig`] / [`super::LaunchPlan`] can
//! consume via [`PackageManifest::to_rootfs_config`] /
//! [`PackageManifest::to_kernel_config`].

pub mod bake_agent;
pub mod canned;
mod hermetic;
#[cfg(feature = "sandbox-rootfs-pack")]
mod live;
pub mod release_asset;
pub mod tools;

pub mod pack_manifest;
pub mod pack_strategy;

pub use bake_agent::{
    resolve_vsock_agent_bin, stage_vsock_agent_into_tree, BakeAgentRequest, BakeAgentResult,
    AGENT_PATH_ENV, DEFAULT_GUEST_AGENT_REL, DEFAULT_GUEST_UNIT_REL, DEFAULT_SYSTEMD_UNIT,
};
pub use canned::{
    build_canned_rootfs, ensure_vsock_agent_bin, guard_against_tmp as canned_guard_against_tmp,
    materialize_base_tree, resolve_canned_out_dir, seed_minimal_rootfs_tree, CannedMode,
    CannedRootfsRequest, CannedRootfsResult, AGENT_CACHE_ENV, AGENT_SHA256_ENV, AGENT_URL_ENV,
    ALT_CANNED_OUT_REL, CANNED_ROOTFS_IMG, CANNED_ROOTFS_OUT_ENV, CANNED_TREE_DIR,
    CANNED_TREE_MANIFEST, DEFAULT_CANNED_OUT_REL,
};
pub use hermetic::HermeticPackBuilder;
pub use pack_manifest::*;
pub use pack_strategy::*;
pub use release_asset::{
    build_and_stage_release_rootfs, checkout_asset_pin,
    durable_cache_dir as rootfs_release_cache_dir, effective_release_pin, ensure_rootfs_release,
    env_asset_pin, filename_from_asset_url, gh_upload_hints, release_canned_request,
    resolve_release_out_dir, resolve_rootfs_release, resolve_rootfs_release_with_roots,
    rootfs_release_sha256_file, stage_release_asset, verify_rootfs_release,
    versioned_image_filename, ReleaseAssetManifest, ReleaseAssetRequest, ReleaseAssetResult,
    RootfsReleasePin, DEFAULT_RELEASE_ARCH, DEFAULT_RELEASE_OUT_REL, RELEASE_ASSET_MANIFEST,
    ROOTFS_ASSET_SHA256_ENV, ROOTFS_ASSET_URL_ENV, ROOTFS_IMG_ENV, ROOTFS_RELEASE_CACHE_ENV,
    ROOTFS_RELEASE_FILENAME, ROOTFS_RELEASE_OUT_ENV, ROOTFS_RELEASE_PUBLISHED,
    ROOTFS_RELEASE_SHA256, ROOTFS_RELEASE_SOURCE, ROOTFS_RELEASE_URL, ROOTFS_RELEASE_VERSION,
};
pub use tools::PackToolSnapshot;

#[cfg(test)]
mod unit_tests;

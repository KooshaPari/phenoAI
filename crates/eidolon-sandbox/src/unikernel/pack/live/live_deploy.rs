//! Live rootfs deploy: compose docker export → tree → Ext4 disk.

use std::fs;
use std::path::Path;

use eidolon_core::error::PhenoError;
use eidolon_core::Result;

use super::live_build::{
    finish_manifest, pack_docker_export, pack_mkfs_ext4, pack_virt_make_fs, require_staging_dir,
};
use super::{
    fill_checksums, maybe_bake_into_tree, pack_io, rootfs_format_label, DiskImageBackend,
    PackMethod, PackRequest, PackageManifest, MANIFEST_FILENAME, MANIFEST_SCHEMA_VERSION,
};
use crate::codes;
use crate::unikernel::RootfsFormat;

/// Compose docker export → tree → Ext4 disk with pre-resolved tools.
pub(super) fn run_docker_to_ext4(
    docker: &Path,
    disk_tool: &Path,
    req: &PackRequest,
) -> Result<PackageManifest> {
    let staging = require_staging_dir(req)?.clone();
    fs::create_dir_all(&staging).map_err(|e| pack_io(&staging, e))?;

    let mut export_req = req.clone();
    export_req.pack_method = PackMethod::DockerExport;
    export_req.rootfs_format = RootfsFormat::Raw;
    export_req.bake_vsock_agent = false;
    let export_manifest = pack_docker_export(docker, &export_req)?;
    let tree = staging.join("rootfs-tree");
    if !tree.is_dir() {
        return Err(PhenoError::Internal(format!(
            "{}: docker_to_ext4 expected rootfs-tree at {} after export (got tar at {})",
            codes::SANDBOX_ROOTFS_PACK_IO,
            tree.display(),
            export_manifest.rootfs.display()
        )));
    }

    maybe_bake_into_tree(&tree, req)?;

    let mut disk_req = req.clone();
    disk_req.rootfs_source = tree;
    disk_req.rootfs_format = RootfsFormat::Ext4;
    disk_req.pack_method = req.disk_backend.as_pack_method();
    disk_req.bake_vsock_agent = false;

    let disk_manifest = match req.disk_backend {
        DiskImageBackend::MkfsExt4 => pack_mkfs_ext4(disk_tool, &disk_req)?,
        DiskImageBackend::VirtMakeFs => pack_virt_make_fs(disk_tool, &disk_req)?,
    };

    let mut manifest = PackageManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        rootfs: disk_manifest.rootfs,
        rootfs_format: rootfs_format_label(RootfsFormat::Ext4).to_string(),
        kernel: disk_manifest.kernel,
        rootfs_checksum: None,
        kernel_checksum: None,
        pack_method: PackMethod::DockerToExt4,
        staging_dir: Some(staging.clone()),
    };
    fill_checksums(&mut manifest, req.compute_checksum)?;
    manifest.write_to(&staging.join(MANIFEST_FILENAME))?;
    Ok(manifest)
}

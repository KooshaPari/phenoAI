//! Hermetic pack: validate existing artifacts, optional stage, write manifest.

use super::{
    file_name_or, fill_checksums, pack_io, require_source_file, rootfs_format_label, stage_artifact,
    PackMethod, PackRequest, PackageManifest, MANIFEST_FILENAME, MANIFEST_SCHEMA_VERSION,
};
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::fs;

/// Hermetic pack: validate sources, optional stage, write manifest JSON.
///
/// Does **not** call `mkfs` / `virt-make-fs` / `docker`. For live image build
/// use [`super::live_pack`] behind [`super::PACK_INTEGRATION_ENV`] + feature
/// `sandbox-rootfs-pack`.
#[derive(Debug, Default, Clone, Copy)]
pub struct HermeticPackBuilder;

impl HermeticPackBuilder {
    pub fn new() -> Self {
        Self
    }

    /// Validate, optionally stage into `staging_dir`, write manifest, return it.
    pub fn build(&self, req: &PackRequest) -> Result<PackageManifest> {
        req.validate()?;
        if req.pack_method != PackMethod::Hermetic {
            return Err(PhenoError::BadRequest(format!(
                "HermeticPackBuilder only accepts PackMethod::Hermetic (got {}); \
                 use live_pack for {}",
                req.pack_method.as_str(),
                req.pack_method.as_str()
            )));
        }

        require_source_file(&req.rootfs_source, "rootfs")?;
        if let Some(k) = &req.kernel {
            require_source_file(k, "kernel")?;
        }

        let (rootfs_path, kernel_path, staging) = match &req.staging_dir {
            Some(dir) => {
                fs::create_dir_all(dir).map_err(|e| pack_io(dir, e))?;
                let staged_rootfs = dir.join(file_name_or(&req.rootfs_source, "rootfs.img"));
                stage_artifact(&req.rootfs_source, &staged_rootfs, req.link_artifacts)?;
                let staged_kernel = if let Some(k) = &req.kernel {
                    let dest = dir.join(file_name_or(k, "vmlinux"));
                    stage_artifact(k, &dest, req.link_artifacts)?;
                    Some(dest)
                } else {
                    None
                };
                (staged_rootfs, staged_kernel, Some(dir.clone()))
            }
            None => (req.rootfs_source.clone(), req.kernel.clone(), None),
        };

        let mut manifest = PackageManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            rootfs: rootfs_path,
            rootfs_format: rootfs_format_label(req.rootfs_format).to_string(),
            kernel: kernel_path,
            rootfs_checksum: None,
            kernel_checksum: None,
            pack_method: PackMethod::Hermetic,
            staging_dir: staging,
        };
        fill_checksums(&mut manifest, req.compute_checksum)?;
        if let Some(dir) = &manifest.staging_dir {
            manifest.write_to(&dir.join(MANIFEST_FILENAME))?;
        }
        Ok(manifest)
    }
}

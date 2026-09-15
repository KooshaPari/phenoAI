//! Live rootfs build helpers: `mkfs.ext4`, `virt-make-fs`, `docker export`.

use std::fs;
use std::path::Path;
use std::process::Command;

use eidolon_core::error::PhenoError;
use eidolon_core::Result;

use super::{
    file_name_or, fill_checksums, maybe_bake_into_tree, pack_io, rootfs_format_label,
    stage_artifact, PackMethod, PackRequest, PackageManifest, DEFAULT_IMAGE_SIZE_BYTES,
    MANIFEST_FILENAME, MANIFEST_SCHEMA_VERSION,
};
use crate::codes;
use crate::unikernel::RootfsFormat;

pub(super) fn require_staging_dir<'a>(req: &'a PackRequest) -> Result<&'a std::path::PathBuf> {
    req.staging_dir.as_ref().ok_or_else(|| {
        PhenoError::BadRequest(format!(
            "live rootfs pack method={} requires staging_dir for output artifacts \
             (fail-loud — no implicit cwd write)",
            req.pack_method.as_str()
        ))
    })
}

pub(super) fn pack_mkfs_ext4(tool: &Path, req: &PackRequest) -> Result<PackageManifest> {
    let staging = require_staging_dir(req)?;
    fs::create_dir_all(staging).map_err(|e| pack_io(staging, e))?;

    let source = &req.rootfs_source;
    let out = staging.join("rootfs.img");
    if out.exists() {
        fs::remove_file(&out).map_err(|e| pack_io(&out, e))?;
    }

    if source.is_dir() {
        maybe_bake_into_tree(source, req)?;
        let size = req.image_size_bytes.unwrap_or(DEFAULT_IMAGE_SIZE_BYTES);
        create_sparse_file(&out, size)?;
        let mut cmd = Command::new(tool);
        cmd.arg("-F").arg("-d").arg(source).arg(&out);
        run_command(
            &mut cmd,
            &format!("mkfs.ext4 -F -d {} {}", source.display(), out.display()),
        )?;
    } else if source.is_file() {
        if req.bake_vsock_agent {
            return Err(PhenoError::BadRequest(
                "bake_vsock_agent requires a rootfs-tree directory source for MkfsExt4 \
                 (got a file); use a directory tree or DockerToExt4 \
                 (docs/guides/vsock-guest-agent.md)"
                    .into(),
            ));
        }
        stage_artifact(source, &out, req.link_artifacts)?;
        let mut cmd = Command::new(tool);
        cmd.arg("-F").arg(&out);
        run_command(&mut cmd, &format!("mkfs.ext4 -F {}", out.display()))?;
    } else {
        return Err(PhenoError::BadRequest(format!(
            "mkfs_ext4 rootfs_source must be an existing directory (populate) or \
             image file (format in place); got {}",
            source.display()
        )));
    }

    finish_manifest(req, out, staging.clone(), RootfsFormat::Ext4)
}

pub(super) fn pack_virt_make_fs(tool: &Path, req: &PackRequest) -> Result<PackageManifest> {
    let staging = require_staging_dir(req)?;
    fs::create_dir_all(staging).map_err(|e| pack_io(staging, e))?;

    let source = &req.rootfs_source;
    if !source.is_dir() {
        return Err(PhenoError::BadRequest(format!(
            "virt_make_fs rootfs_source must be a directory tree; got {}",
            source.display()
        )));
    }

    maybe_bake_into_tree(source, req)?;

    let out = staging.join("rootfs.img");
    if out.exists() {
        fs::remove_file(&out).map_err(|e| pack_io(&out, e))?;
    }

    let fs_type = match req.rootfs_format {
        RootfsFormat::Ext4 | RootfsFormat::Unspecified | RootfsFormat::Raw => "ext4",
        other => {
            return Err(PhenoError::BadRequest(format!(
                "virt_make_fs unsupported rootfs_format={} (use ext4/raw)",
                rootfs_format_label(other)
            )));
        }
    };

    let mut cmd = Command::new(tool);
    cmd.arg(format!("--type={fs_type}"))
        .arg("--format=raw")
        .arg(source)
        .arg(&out);
    run_command(
        &mut cmd,
        &format!(
            "virt-make-fs --type={fs_type} --format=raw {} {}",
            source.display(),
            out.display()
        ),
    )?;

    if !out.is_file() {
        return Err(PhenoError::Internal(format!(
            "{}: virt-make-fs reported success but output missing at {}",
            codes::SANDBOX_ROOTFS_PACK_IO,
            out.display()
        )));
    }

    finish_manifest(req, out, staging.clone(), RootfsFormat::Ext4)
}

pub(super) fn pack_docker_export(tool: &Path, req: &PackRequest) -> Result<PackageManifest> {
    let staging = require_staging_dir(req)?;
    fs::create_dir_all(staging).map_err(|e| pack_io(staging, e))?;

    if matches!(req.rootfs_format, RootfsFormat::Ext4) {
        return Err(PhenoError::BadRequest(
            "docker_export refuses RootfsFormat::Ext4 — tarball is Raw only; \
             use PackMethod::DockerToExt4 for Ext4 disk images"
                .into(),
        ));
    }

    let image_or_container = req.rootfs_source.to_string_lossy();
    let ref_name = image_or_container.trim();
    if ref_name.is_empty() {
        return Err(PhenoError::BadRequest(
            "docker_export rootfs_source must be a non-empty image or container ref".into(),
        ));
    }

    let out = staging.join("rootfs.tar");
    if out.exists() {
        fs::remove_file(&out).map_err(|e| pack_io(&out, e))?;
    }

    if docker_is_container(tool, ref_name)? {
        let mut cmd = Command::new(tool);
        cmd.arg("export").arg("-o").arg(&out).arg(ref_name);
        run_command(
            &mut cmd,
            &format!("docker export -o {} {ref_name}", out.display()),
        )?;
    } else {
        let tmp_name = format!(
            "eidolon-pack-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        );
        let mut create = Command::new(tool);
        create
            .arg("create")
            .arg("--name")
            .arg(&tmp_name)
            .arg(ref_name);
        run_command(
            &mut create,
            &format!("docker create --name {tmp_name} {ref_name}"),
        )?;

        let export_result = {
            let mut export = Command::new(tool);
            export.arg("export").arg("-o").arg(&out).arg(&tmp_name);
            run_command(
                &mut export,
                &format!("docker export -o {} {tmp_name}", out.display()),
            )
        };

        let mut rm = Command::new(tool);
        rm.arg("rm").arg(&tmp_name);
        let rm_result = run_command(&mut rm, &format!("docker rm {tmp_name}"));

        export_result?;
        rm_result?;
    }

    if !out.is_file() {
        return Err(PhenoError::Internal(format!(
            "{}: docker export reported success but tarball missing at {}",
            codes::SANDBOX_ROOTFS_PACK_IO,
            out.display()
        )));
    }

    let tree = staging.join("rootfs-tree");
    if tree.exists() {
        fs::remove_dir_all(&tree).map_err(|e| pack_io(&tree, e))?;
    }
    fs::create_dir_all(&tree).map_err(|e| pack_io(&tree, e))?;
    let mut untar = Command::new("tar");
    untar.arg("-xf").arg(&out).arg("-C").arg(&tree);
    run_command(
        &mut untar,
        &format!("tar -xf {} -C {}", out.display(), tree.display()),
    )?;

    finish_manifest(req, out, staging.clone(), RootfsFormat::Raw)
}

fn docker_is_container(tool: &Path, name: &str) -> Result<bool> {
    let mut cmd = Command::new(tool);
    cmd.arg("inspect")
        .arg("--format")
        .arg("{{.State.Status}}")
        .arg(name);
    match cmd.output() {
        Ok(out) if out.status.success() => Ok(true),
        Ok(_) => Ok(false),
        Err(e) => Err(PhenoError::Internal(format!(
            "{}: docker inspect failed to spawn: {e}",
            codes::SANDBOX_ROOTFS_PACK_IO
        ))),
    }
}

fn create_sparse_file(path: &Path, size: u64) -> Result<()> {
    let f = fs::File::create(path).map_err(|e| pack_io(path, e))?;
    f.set_len(size).map_err(|e| pack_io(path, e))?;
    Ok(())
}

pub(super) fn run_command(cmd: &mut Command, context: &str) -> Result<()> {
    let output = cmd.output().map_err(|e| {
        PhenoError::Internal(format!(
            "{}: failed to spawn `{context}`: {e}",
            codes::SANDBOX_ROOTFS_PACK_IO
        ))
    })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(PhenoError::Internal(format!(
        "{}: `{context}` failed (exit {:?}): stderr={} stdout={}",
        codes::SANDBOX_ROOTFS_PACK_IO,
        output.status.code(),
        stderr.trim(),
        stdout.trim()
    )))
}

pub(super) fn finish_manifest(
    req: &PackRequest,
    rootfs: std::path::PathBuf,
    staging: std::path::PathBuf,
    format: RootfsFormat,
) -> Result<PackageManifest> {
    let kernel_path = if let Some(k) = &req.kernel {
        if !k.is_file() {
            return Err(PhenoError::unsupported_platform(
                codes::SANDBOX_KERNEL_MISSING,
                format!(
                    "rootfs pack kernel missing or not a file at {} — fail-loud",
                    k.display()
                ),
            ));
        }
        let dest = staging.join(file_name_or(k, "vmlinux"));
        stage_artifact(k, &dest, req.link_artifacts)?;
        Some(dest)
    } else {
        None
    };

    let mut manifest = PackageManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        rootfs,
        rootfs_format: rootfs_format_label(format).to_string(),
        kernel: kernel_path,
        rootfs_checksum: None,
        kernel_checksum: None,
        pack_method: req.pack_method,
        staging_dir: Some(staging.clone()),
    };
    fill_checksums(&mut manifest, req.compute_checksum)?;
    manifest.write_to(&staging.join(MANIFEST_FILENAME))?;
    Ok(manifest)
}

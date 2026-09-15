//! Live rootfs pack pipelines: `mkfs.ext4`, `virt-make-fs`, `docker export`,
//! and compose `docker export` → Ext4 disk ([`PackMethod::DockerToExt4`]).
//!
//! Invoked only from [`super::live_pack`] / [`super::compose_docker_to_ext4`]
//! when feature `sandbox-rootfs-pack` is on, [`super::PACK_INTEGRATION_ENV`]
//! is `"1"`, and host tools resolved.

mod live_build;
mod live_deploy;

use eidolon_core::error::PhenoError;
use eidolon_core::Result;

pub(super) use super::{
    file_name_or, fill_checksums, maybe_bake_into_tree, pack_io, rootfs_format_label,
    stage_artifact, DiskImageBackend, PackMethod, PackRequest, PackageManifest,
    DEFAULT_IMAGE_SIZE_BYTES, MANIFEST_FILENAME, MANIFEST_SCHEMA_VERSION,
};

/// Run a live pack with an already-resolved host tool binary.
pub(super) fn run_live_pack(tool: &std::path::Path, req: &PackRequest) -> Result<PackageManifest> {
    live_build::require_staging_dir(req)?;
    match req.pack_method {
        PackMethod::Hermetic => Err(PhenoError::BadRequest(
            "run_live_pack does not accept PackMethod::Hermetic".into(),
        )),
        PackMethod::DockerToExt4 => Err(PhenoError::BadRequest(
            "run_live_pack does not accept PackMethod::DockerToExt4 — use \
             run_docker_to_ext4 / compose_docker_to_ext4 (dual-tool)"
                .into(),
        )),
        PackMethod::MkfsExt4 => live_build::pack_mkfs_ext4(tool, req),
        PackMethod::VirtMakeFs => live_build::pack_virt_make_fs(tool, req),
        PackMethod::DockerExport => live_build::pack_docker_export(tool, req),
    }
}

pub(super) use live_deploy::run_docker_to_ext4;

#[cfg(all(test, feature = "sandbox-rootfs-pack"))]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use super::*;
    use crate::unikernel::RootfsFormat;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "eidolon-live-pack-{}-{}-{}",
            name,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("mkdir");
        path
    }

    fn write_executable(path: &Path, body: &str) {
        let mut f = fs::File::create(path).expect("create script");
        writeln!(f, "#!/bin/sh").unwrap();
        write!(f, "{body}").unwrap();
        drop(f);
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    fn fake_docker_script() -> &'static str {
        r#"
cmd="$1"
shift
case "$cmd" in
  inspect) exit 1 ;;
  create) exit 0 ;;
  export)
    out=""
    while [ $# -gt 0 ]; do
      if [ "$1" = "-o" ]; then out="$2"; shift 2; continue; fi
      shift
    done
    if command -v tar >/dev/null 2>&1; then
      emptydir=$(mktemp -d)
      echo compose-marker > "$emptydir/hello"
      tar -cf "$out" -C "$emptydir" .
      rm -rf "$emptydir"
    else
      : > "$out"
    fi
    exit 0
    ;;
  rm) exit 0 ;;
  *) echo "unexpected: $cmd" >&2; exit 1 ;;
esac
"#
    }

    fn fake_mkfs_script() -> &'static str {
        r#"
out=""
for a in "$@"; do out="$a"; done
if [ ! -f "$out" ]; then : > "$out"; fi
exit 0
"#
    }

    #[test]
    fn fake_mkfs_formats_directory_tree() {
        let _g = ENV_LOCK.lock().unwrap();
        let root = temp_dir("mkfs-src");
        fs::write(root.join("hello"), b"world").unwrap();
        let staging = temp_dir("mkfs-stage");
        let bin = staging.join("fake-mkfs");
        write_executable(&bin, fake_mkfs_script());

        let req = PackRequest::hermetic(&root, RootfsFormat::Ext4)
            .with_method(PackMethod::MkfsExt4)
            .with_staging(&staging, false)
            .with_image_size(1024 * 1024);
        let manifest = run_live_pack(&bin, &req).expect("mkfs live");
        assert_eq!(manifest.pack_method, PackMethod::MkfsExt4);
        assert_eq!(manifest.rootfs_format, "ext4");
        assert!(manifest.rootfs.is_file());
        assert!(staging.join(MANIFEST_FILENAME).is_file());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&staging);
    }

    #[test]
    fn fake_mkfs_bakes_agent_then_packs() {
        let _g = ENV_LOCK.lock().unwrap();
        let root = temp_dir("mkfs-bake-src");
        fs::write(root.join("hello"), b"world").unwrap();
        let staging = temp_dir("mkfs-bake-stage");
        let agent_host = staging.join("fake-agent-bin");
        write_executable(&agent_host, "echo agent\n");
        let bin = staging.join("fake-mkfs");
        write_executable(&bin, fake_mkfs_script());

        let req = PackRequest::hermetic(&root, RootfsFormat::Ext4)
            .with_method(PackMethod::MkfsExt4)
            .with_staging(&staging, false)
            .with_image_size(1024 * 1024)
            .with_bake_vsock_agent(true)
            .with_vsock_agent_bin(&agent_host)
            .with_bake_systemd_unit(true);
        let manifest = run_live_pack(&bin, &req).expect("mkfs+bake");
        assert_eq!(manifest.pack_method, PackMethod::MkfsExt4);
        assert!(root
            .join(crate::unikernel::pack::DEFAULT_GUEST_AGENT_REL)
            .is_file());
        assert!(root
            .join(crate::unikernel::pack::DEFAULT_GUEST_UNIT_REL)
            .is_file());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&staging);
    }

    #[test]
    fn fake_mkfs_bake_missing_agent_fail_loud() {
        let root = temp_dir("mkfs-bake-missing");
        fs::write(root.join("hello"), b"world").unwrap();
        let staging = temp_dir("mkfs-bake-missing-stage");
        let bin = staging.join("fake-mkfs");
        write_executable(&bin, fake_mkfs_script());
        let req = PackRequest::hermetic(&root, RootfsFormat::Ext4)
            .with_method(PackMethod::MkfsExt4)
            .with_staging(&staging, false)
            .with_bake_vsock_agent(true)
            .with_vsock_agent_bin("/no/such/eidolon-vsock-agent-bake");
        let err = run_live_pack(&bin, &req).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_VSOCK_AGENT_MISSING)
        );
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&staging);
    }

    #[test]
    fn fake_docker_to_ext4_bakes_agent_into_tree() {
        let _g = ENV_LOCK.lock().unwrap();
        let staging = temp_dir("compose-bake");
        let docker = staging.join("fake-docker");
        let mkfs = staging.join("fake-mkfs");
        let agent_host = staging.join("fake-agent-bin");
        write_executable(&docker, fake_docker_script());
        write_executable(&mkfs, fake_mkfs_script());
        write_executable(&agent_host, "echo agent\n");

        let req = PackRequest::hermetic("alpine:3.19", RootfsFormat::Ext4)
            .with_method(PackMethod::DockerToExt4)
            .with_staging(&staging, false)
            .with_image_size(1024 * 1024)
            .with_bake_vsock_agent(true)
            .with_vsock_agent_bin(&agent_host);
        let manifest = run_docker_to_ext4(&docker, &mkfs, &req).expect("compose+bake");
        assert_eq!(manifest.pack_method, PackMethod::DockerToExt4);
        assert!(staging
            .join("rootfs-tree")
            .join(crate::unikernel::pack::DEFAULT_GUEST_AGENT_REL)
            .is_file());
        let _ = fs::remove_dir_all(&staging);
    }

    #[test]
    fn fake_virt_make_fs_writes_image() {
        let _g = ENV_LOCK.lock().unwrap();
        let root = temp_dir("virt-src");
        fs::write(root.join("a"), b"b").unwrap();
        let staging = temp_dir("virt-stage");
        let bin = staging.join("fake-virt-make-fs");
        write_executable(
            &bin,
            r#"
out=""
for a in "$@"; do out="$a"; done
: > "$out"
exit 0
"#,
        );

        let req = PackRequest::hermetic(&root, RootfsFormat::Ext4)
            .with_method(PackMethod::VirtMakeFs)
            .with_staging(&staging, false);
        let manifest = run_live_pack(&bin, &req).expect("virt live");
        assert_eq!(manifest.pack_method, PackMethod::VirtMakeFs);
        assert_eq!(manifest.rootfs_format, "ext4");
        assert!(manifest.rootfs.is_file());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&staging);
    }

    #[test]
    fn fake_docker_export_writes_tarball_as_raw() {
        let _g = ENV_LOCK.lock().unwrap();
        let staging = temp_dir("docker-stage");
        let bin = staging.join("fake-docker");
        write_executable(&bin, fake_docker_script());

        let req = PackRequest::hermetic("alpine:3.19", RootfsFormat::Raw)
            .with_method(PackMethod::DockerExport)
            .with_staging(&staging, false);
        let manifest = run_live_pack(&bin, &req).expect("docker live");
        assert_eq!(manifest.pack_method, PackMethod::DockerExport);
        assert_eq!(manifest.rootfs_format, "raw", "never claim Ext4 for tar");
        assert!(manifest.rootfs.is_file());
        assert!(staging.join("rootfs-tree").is_dir());
        let cfg = manifest.to_rootfs_config().expect("rootfs cfg");
        assert_eq!(cfg.format, RootfsFormat::Raw);
        assert!(cfg.path.is_file());
        let _ = fs::remove_dir_all(&staging);
    }

    #[test]
    fn fake_docker_export_rejects_ext4_claim() {
        let staging = temp_dir("docker-ext4-bad");
        let bin = staging.join("fake-docker");
        write_executable(&bin, fake_docker_script());
        let req = PackRequest::hermetic("alpine:3.19", RootfsFormat::Ext4)
            .with_method(PackMethod::DockerExport)
            .with_staging(&staging, false);
        let err = req.validate().unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
        let err2 = run_live_pack(&bin, &req).unwrap_err();
        assert!(matches!(err2, PhenoError::BadRequest(_)));
        let _ = fs::remove_dir_all(&staging);
    }

    #[test]
    fn fake_docker_to_ext4_compose_writes_disk() {
        let _g = ENV_LOCK.lock().unwrap();
        let staging = temp_dir("compose-stage");
        let docker = staging.join("fake-docker");
        let mkfs = staging.join("fake-mkfs");
        write_executable(&docker, fake_docker_script());
        write_executable(&mkfs, fake_mkfs_script());

        let req = PackRequest::hermetic("alpine:3.19", RootfsFormat::Ext4)
            .with_method(PackMethod::DockerToExt4)
            .with_staging(&staging, false)
            .with_image_size(1024 * 1024);
        let manifest = run_docker_to_ext4(&docker, &mkfs, &req).expect("compose");
        assert_eq!(manifest.pack_method, PackMethod::DockerToExt4);
        assert_eq!(manifest.rootfs_format, "ext4");
        assert!(manifest.rootfs.ends_with("rootfs.img"));
        assert!(manifest.rootfs.is_file());
        assert!(staging.join("rootfs.tar").is_file());
        assert!(staging.join("rootfs-tree").is_dir());
        let cfg = manifest.to_rootfs_config().expect("LaunchPlan rootfs");
        assert_eq!(cfg.format, RootfsFormat::Ext4);
        let _ = fs::remove_dir_all(&staging);
    }

    #[test]
    fn fake_docker_to_ext4_via_virt_make_fs() {
        let _g = ENV_LOCK.lock().unwrap();
        let staging = temp_dir("compose-virt");
        let docker = staging.join("fake-docker");
        let virt = staging.join("fake-virt");
        write_executable(&docker, fake_docker_script());
        write_executable(
            &virt,
            r#"
out=""
for a in "$@"; do out="$a"; done
: > "$out"
exit 0
"#,
        );

        let req = PackRequest::hermetic("busybox:latest", RootfsFormat::Ext4)
            .with_method(PackMethod::DockerToExt4)
            .with_disk_backend(DiskImageBackend::VirtMakeFs)
            .with_staging(&staging, false);
        let manifest = run_docker_to_ext4(&docker, &virt, &req).expect("compose virt");
        assert_eq!(manifest.pack_method, PackMethod::DockerToExt4);
        assert_eq!(manifest.rootfs_format, "ext4");
        assert!(manifest.rootfs.is_file());
        let _ = fs::remove_dir_all(&staging);
    }

    #[test]
    fn live_pack_requires_staging() {
        let bin = temp_dir("nostage").join("noop");
        write_executable(&bin, "exit 0\n");
        let root = temp_dir("src");
        let req =
            PackRequest::hermetic(&root, RootfsFormat::Ext4).with_method(PackMethod::VirtMakeFs);
        let err = run_live_pack(&bin, &req).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_file(&bin);
    }
}

use super::*;
use crate::codes;
use eidolon_core::error::PhenoError;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::unikernel::RootfsFormat;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn temp_file(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "eidolon-pack-{}-{}-{}",
        name,
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let mut f = fs::File::create(&path).expect("create");
    writeln!(f, "eidolon-pack-fixture-{name}").expect("write");
    path
}

fn temp_dir(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "eidolon-pack-dir-{}-{}-{}",
        name,
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&path).expect("mkdir");
    path
}

#[test]
fn hermetic_build_writes_manifest_and_composes_rootfs_config() {
    let rootfs = temp_file("rootfs");
    let kernel = temp_file("kernel");
    let staging = temp_dir("stage");
    let req = PackRequest::hermetic(&rootfs, RootfsFormat::Ext4)
        .with_kernel(&kernel)
        .with_staging(&staging, false);
    let manifest = HermeticPackBuilder::new().build(&req).expect("hermetic");
    assert_eq!(manifest.pack_method, PackMethod::Hermetic);
    assert!(manifest.rootfs.starts_with(&staging));
    assert!(staging.join(MANIFEST_FILENAME).is_file());
    let cfg = manifest.to_rootfs_config().expect("rootfs cfg");
    assert_eq!(cfg.format, RootfsFormat::Ext4);
    cfg.require_present().expect("staged rootfs present");
    let k = manifest.to_kernel_config().unwrap().expect("kernel");
    k.require_present().expect("staged kernel");
    let loaded = PackageManifest::read_from(&staging.join(MANIFEST_FILENAME)).unwrap();
    assert_eq!(loaded.schema_version, MANIFEST_SCHEMA_VERSION);
    let _ = fs::remove_file(&rootfs);
    let _ = fs::remove_file(&kernel);
    let _ = fs::remove_dir_all(&staging);
}

#[test]
fn hermetic_missing_rootfs_fail_loud() {
    let req = PackRequest::hermetic("/no/such/eidolon-pack-rootfs.img", RootfsFormat::Raw);
    let err = HermeticPackBuilder::new().build(&req).unwrap_err();
    assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_ROOTFS_MISSING));
}

#[test]
fn live_pack_without_integration_fail_loud() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::remove_var(PACK_INTEGRATION_ENV);
    let rootfs = temp_file("live-src");
    let req = PackRequest::hermetic(&rootfs, RootfsFormat::Ext4).with_method(PackMethod::MkfsExt4);
    let err = live_pack(&req).unwrap_err();
    assert!(
        err.unsupported_code() == Some(codes::SANDBOX_ROOTFS_PACK_STUB)
            || err.unsupported_code() == Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING)
    );
    let _ = fs::remove_file(&rootfs);
}

#[test]
fn tool_snapshot_is_consistent() {
    let snap = tools::tool_snapshot();
    assert_eq!(
        tools::pack_tool_ready(PackMethod::MkfsExt4),
        snap.mkfs.is_some()
    );
    assert_eq!(
        tools::pack_tool_ready(PackMethod::VirtMakeFs),
        snap.virt_make_fs.is_some()
    );
    assert_eq!(
        tools::pack_tool_ready(PackMethod::DockerExport),
        snap.docker.is_some()
    );
    assert_eq!(
        tools::pack_tool_ready(PackMethod::DockerToExt4),
        snap.docker_to_ext4_ready()
    );
    assert!(tools::pack_tool_ready(PackMethod::Hermetic));
}

#[test]
fn docker_export_ext4_claim_fail_loud() {
    let req = PackRequest::hermetic("alpine:3.19", RootfsFormat::Ext4)
        .with_method(PackMethod::DockerExport);
    let err = req.validate().unwrap_err();
    assert!(
        matches!(err, PhenoError::BadRequest(_)),
        "must refuse Ext4 claim for tar-only export: {err:?}"
    );
}

#[test]
fn empty_paths_bad_request() {
    let req = PackRequest::hermetic("", RootfsFormat::Unspecified);
    assert!(matches!(req.validate(), Err(PhenoError::BadRequest(_))));
}

#[test]
fn bake_with_hermetic_method_fail_loud() {
    let req = PackRequest::hermetic("rootfs.img", RootfsFormat::Ext4).with_bake_vsock_agent(true);
    let err = req.validate().unwrap_err();
    assert!(matches!(err, PhenoError::BadRequest(_)));
}

#[test]
fn bake_agent_then_pack_rejects_hermetic() {
    let tree = temp_dir("bake-hermetic");
    let req = PackRequest::hermetic(&tree, RootfsFormat::Ext4);
    let err = bake_agent_then_pack(&req).unwrap_err();
    assert!(matches!(err, PhenoError::BadRequest(_)));
    let _ = fs::remove_dir_all(&tree);
}

#[cfg(feature = "sandbox-rootfs-pack")]
#[test]
fn checksum_with_feature() {
    let rootfs = temp_file("cksum");
    let req = PackRequest::hermetic(&rootfs, RootfsFormat::Ext4).with_checksum(true);
    let manifest = HermeticPackBuilder::new().build(&req).expect("cksum");
    let ck = manifest.rootfs_checksum.expect("checksum");
    assert_eq!(ck.algorithm, ChecksumAlgorithm::Sha256);
    assert_eq!(ck.hex.len(), 64);
    let _ = fs::remove_file(&rootfs);
}

#[cfg(feature = "sandbox-rootfs-pack")]
    #[test]
    fn live_pack_tool_missing_with_integration() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var(PACK_INTEGRATION_ENV, "1");
        // Point overrides at non-existent paths so PATH tools are ignored.
        std::env::set_var(tools::MKFS_PATH_ENV, "/no/such/eidolon-mkfs-bin");
        std::env::set_var(tools::VIRT_MAKE_FS_PATH_ENV, "/no/such/eidolon-virt-make-fs");
        std::env::set_var(tools::DOCKER_PATH_ENV, "/no/such/eidolon-docker");

        let staging = temp_dir("missing-tool");
        let tree = temp_dir("missing-tree");
        let err = live_pack(
            &PackRequest::hermetic(&tree, RootfsFormat::Ext4)
                .with_method(PackMethod::VirtMakeFs)
                .with_staging(&staging, false),
        )
        .unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING)
        );

        let compose_err = live_pack(
            &PackRequest::hermetic("alpine:3.19", RootfsFormat::Ext4)
                .with_method(PackMethod::DockerToExt4)
                .with_staging(&staging, false),
        )
        .unwrap_err();
        assert_eq!(
            compose_err.unsupported_code(),
            Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING),
            "compose must fail-loud when docker/mkfs missing"
        );

        std::env::remove_var(PACK_INTEGRATION_ENV);
        std::env::remove_var(tools::MKFS_PATH_ENV);
        std::env::remove_var(tools::VIRT_MAKE_FS_PATH_ENV);
        std::env::remove_var(tools::DOCKER_PATH_ENV);
        let _ = fs::remove_dir_all(&staging);
        let _ = fs::remove_dir_all(&tree);
    }

//! Rootfs / guest image packaging tests (hermetic + gated live).
//!
//! Default `cargo test --locked` stays green without `mkfs`, `virt-make-fs`,
//! or Docker. Feature `sandbox-rootfs-pack` exercises SHA-256 checksums and
//! fake-tool live pipelines (unit). Real tool invocation:
//! `ROOTFS_PACK_INTEGRATION=1` (+ present host tools).

use eidolon_core::error::PhenoError;
use eidolon_sandbox::codes;
use eidolon_sandbox::unikernel_pack::{
    self, build_canned_rootfs, HermeticPackBuilder, PackMethod, PackRequest, PackageManifest,
    CannedMode, CannedRootfsRequest, CANNED_TREE_MANIFEST, MANIFEST_FILENAME, PACK_INTEGRATION_ENV,
};
use eidolon_sandbox::{LaunchPlan, RootfsFormat, UnikernelBackend, UnikernelLaunchConfig};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn temp_file(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "eidolon-pack-it-{}-{}-{}",
        name,
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let mut f = fs::File::create(&path).expect("create");
    writeln!(f, "eidolon-pack-it-{name}").expect("write");
    path
}

fn temp_dir(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "eidolon-pack-it-dir-{}-{}-{}",
        name,
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&path).expect("mkdir");
    path
}

fn assert_code(err: PhenoError, expected: &str) {
    assert_eq!(err.unsupported_code(), Some(expected));
    assert_eq!(err.status_code(), 501);
}

fn durable_temp_dir(name: &str) -> PathBuf {
    // Canned API rejects /tmp — use workspace target/.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/canned-rootfs-it");
    let path = root.join(format!(
        "{}-{}-{}",
        name,
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&path).expect("mkdir durable");
    path
}

fn fake_agent_bin(dir: &PathBuf) -> PathBuf {
    let p = dir.join("fake-eidolon-vsock-agent");
    let mut f = fs::File::create(&p).expect("create agent");
    writeln!(f, "#!/bin/sh\necho fake-it-agent").unwrap();
    drop(f);
    let mut perms = fs::metadata(&p).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&p, perms).unwrap();
    p
}

#[test]
fn canned_tree_only_hermetic_fixture() {
    let out = durable_temp_dir("canned-tree");
    let host = durable_temp_dir("canned-host");
    let agent = fake_agent_bin(&host);

    let result = build_canned_rootfs(
        &CannedRootfsRequest::new()
            .with_out_dir(&out)
            .with_agent_bin(&agent)
            .with_mode(CannedMode::TreeOnly)
            .with_systemd_unit(true),
    )
    .expect("canned tree_only");

    assert!(result.rootfs_img.is_none());
    assert!(result.bake.agent_guest_path.is_file());
    assert!(out.join(CANNED_TREE_MANIFEST).is_file());
    assert_eq!(result.manifest.pack_method, PackMethod::Hermetic);
    assert_eq!(result.manifest.rootfs_format, "raw");

    let _ = fs::remove_dir_all(&out);
    let _ = fs::remove_dir_all(&host);
}

#[test]
fn canned_pack_ext4_gated_without_integration() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::remove_var(PACK_INTEGRATION_ENV);
    let out = durable_temp_dir("canned-pack-gate");
    let host = durable_temp_dir("canned-pack-host");
    let agent = fake_agent_bin(&host);

    let err = build_canned_rootfs(
        &CannedRootfsRequest::new()
            .with_out_dir(&out)
            .with_agent_bin(&agent)
            .with_mode(CannedMode::PackExt4),
    )
    .unwrap_err();
    assert!(
        err.unsupported_code() == Some(codes::SANDBOX_ROOTFS_PACK_STUB)
            || err.unsupported_code() == Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING),
        "expected gated pack, got {err:?}"
    );

    let _ = fs::remove_dir_all(&out);
    let _ = fs::remove_dir_all(&host);
}

#[test]
fn hermetic_pack_stages_and_feeds_launch_plan() {
    let rootfs = temp_file("rootfs");
    let kernel = temp_file("kernel");
    let staging = temp_dir("stage");

    let manifest = HermeticPackBuilder::new()
        .build(
            &PackRequest::hermetic(&rootfs, RootfsFormat::Ext4)
                .with_kernel(&kernel)
                .with_staging(&staging, true),
        )
        .expect("hermetic pack");

    assert!(staging.join(MANIFEST_FILENAME).is_file());
    let rootfs_cfg = manifest.to_rootfs_config().unwrap();
    let kernel_cfg = manifest.to_kernel_config().unwrap().unwrap();

    let cfg = UnikernelLaunchConfig::try_new(
        "pack-plan",
        UnikernelBackend::Firecracker,
        rootfs_cfg,
    )
    .unwrap()
    .with_kernel(kernel_cfg)
    .unwrap();

    match LaunchPlan::try_from_config(&cfg) {
        Ok(plan) => {
            assert_eq!(plan.rootfs, manifest.rootfs);
            assert_eq!(plan.rootfs_format, RootfsFormat::Ext4);
        }
        Err(err) => {
            // Host without firecracker: fail-loud stub, not silent Ok.
            assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_UNIKERNEL_STUB));
        }
    }

    let loaded = PackageManifest::read_from(&staging.join(MANIFEST_FILENAME)).unwrap();
    assert_eq!(loaded.pack_method, PackMethod::Hermetic);
    assert_eq!(loaded.schema_version, 1);

    let _ = fs::remove_file(&rootfs);
    let _ = fs::remove_file(&kernel);
    let _ = fs::remove_dir_all(&staging);
}

#[test]
fn hermetic_pack_missing_source_fail_loud() {
    let err = HermeticPackBuilder::new()
        .build(&PackRequest::hermetic(
            "/no/such/eidolon-pack-it.img",
            RootfsFormat::Raw,
        ))
        .unwrap_err();
    assert_code(err, codes::SANDBOX_ROOTFS_MISSING);
}

#[test]
fn live_mkfs_without_env_fail_loud() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::remove_var(PACK_INTEGRATION_ENV);
    let rootfs = temp_file("live");
    let err = unikernel_pack::live_pack(
        &PackRequest::hermetic(&rootfs, RootfsFormat::Ext4).with_method(PackMethod::MkfsExt4),
    )
    .unwrap_err();
    assert!(
        err.unsupported_code() == Some(codes::SANDBOX_ROOTFS_PACK_STUB)
            || err.unsupported_code() == Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING),
        "unexpected code {:?}",
        err.unsupported_code()
    );
    let _ = fs::remove_file(&rootfs);
}

#[test]
fn pack_tool_probes_are_boolean() {
    let snap = unikernel_pack::tools::tool_snapshot();
    assert_eq!(
        unikernel_pack::tools::pack_tool_ready(PackMethod::DockerExport),
        snap.docker.is_some()
    );
    assert!(unikernel_pack::tools::pack_tool_ready(PackMethod::Hermetic));
}

#[test]
fn documented_pack_codes_stable() {
    assert_eq!(
        codes::SANDBOX_ROOTFS_PACK_STUB,
        "EIDOLON_SANDBOX_ROOTFS_PACK_STUB"
    );
    assert_eq!(
        codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING,
        "EIDOLON_SANDBOX_ROOTFS_PACK_TOOL_MISSING"
    );
    assert_eq!(codes::SANDBOX_ROOTFS_PACK_IO, "EIDOLON_SANDBOX_ROOTFS_PACK_IO");
    assert_eq!(
        codes::SANDBOX_VSOCK_AGENT_MISSING,
        "EIDOLON_SANDBOX_VSOCK_AGENT_MISSING"
    );
}

#[cfg(feature = "sandbox-rootfs-pack")]
mod feature_tests {
    use super::*;
    use eidolon_sandbox::unikernel_pack::tools::{
        DOCKER_PATH_ENV, MKFS_PATH_ENV, VIRT_MAKE_FS_PATH_ENV,
    };
    use eidolon_sandbox::unikernel_pack::ChecksumAlgorithm;

    #[test]
    fn checksum_and_integration_gate_tool_missing() {
        let _g = ENV_LOCK.lock().unwrap();
        let rootfs = temp_file("cksum-it");
        let manifest = HermeticPackBuilder::new()
            .build(
                &PackRequest::hermetic(&rootfs, RootfsFormat::Ext4).with_checksum(true),
            )
            .expect("checksum");
        let ck = manifest.rootfs_checksum.expect("sha256");
        assert_eq!(ck.algorithm, ChecksumAlgorithm::Sha256);
        assert_eq!(ck.hex.len(), 64);

        std::env::set_var(PACK_INTEGRATION_ENV, "1");
        std::env::set_var(VIRT_MAKE_FS_PATH_ENV, "/no/such/eidolon-virt-make-fs-it");
        let staging = temp_dir("virt-missing");
        let tree = temp_dir("virt-tree");
        let live_err = unikernel_pack::live_pack(
            &PackRequest::hermetic(&tree, RootfsFormat::Ext4)
                .with_method(PackMethod::VirtMakeFs)
                .with_staging(&staging, false),
        )
        .unwrap_err();
        assert_eq!(
            live_err.unsupported_code(),
            Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING),
            "expected TOOL_MISSING, got {:?}",
            live_err.unsupported_code()
        );
        std::env::remove_var(PACK_INTEGRATION_ENV);
        std::env::remove_var(VIRT_MAKE_FS_PATH_ENV);
        let _ = fs::remove_file(&rootfs);
        let _ = fs::remove_dir_all(&staging);
        let _ = fs::remove_dir_all(&tree);
    }

    /// Live host-tool integration (skipped unless `ROOTFS_PACK_INTEGRATION=1`).
    ///
    /// Exercises whichever of mkfs / virt-make-fs / docker is discoverable.
    /// Failures must be loud (`PACK_IO` / `TOOL_MISSING`), never silent Ok.
    #[test]
    fn live_host_tools_when_integration_env_set() {
        if std::env::var(PACK_INTEGRATION_ENV).ok().as_deref() != Some("1") {
            return;
        }
        let _g = ENV_LOCK.lock().unwrap();
        // Do not poison PATH with unit-test overrides.
        std::env::remove_var(MKFS_PATH_ENV);
        std::env::remove_var(VIRT_MAKE_FS_PATH_ENV);
        std::env::remove_var(DOCKER_PATH_ENV);

        let staging = temp_dir("live-it");
        let tree = temp_dir("live-tree");
        fs::write(tree.join("marker"), b"eidolon-pack-it").unwrap();

        let mut exercised = false;

        if unikernel_pack::tools::pack_tool_ready(PackMethod::VirtMakeFs) {
            exercised = true;
            match unikernel_pack::live_pack(
                &PackRequest::hermetic(&tree, RootfsFormat::Ext4)
                    .with_method(PackMethod::VirtMakeFs)
                    .with_staging(&staging, false),
            ) {
                Ok(m) => {
                    assert_eq!(m.pack_method, PackMethod::VirtMakeFs);
                    assert!(m.rootfs.is_file());
                    let _ = m.to_rootfs_config().expect("LaunchPlan-ready rootfs");
                }
                Err(err) => {
                    assert!(
                        err.unsupported_code() == Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING)
                            || err.to_string().contains(codes::SANDBOX_ROOTFS_PACK_IO),
                        "unexpected live virt-make-fs error: {err:?}"
                    );
                }
            }
        }

        if unikernel_pack::tools::pack_tool_ready(PackMethod::MkfsExt4) {
            exercised = true;
            let mkfs_stage = temp_dir("live-mkfs");
            match unikernel_pack::live_pack(
                &PackRequest::hermetic(&tree, RootfsFormat::Ext4)
                    .with_method(PackMethod::MkfsExt4)
                    .with_staging(&mkfs_stage, false)
                    .with_image_size(8 * 1024 * 1024),
            ) {
                Ok(m) => {
                    assert_eq!(m.pack_method, PackMethod::MkfsExt4);
                    assert!(m.rootfs.is_file());
                }
                Err(err) => {
                    assert!(
                        err.to_string().contains(codes::SANDBOX_ROOTFS_PACK_IO)
                            || err.unsupported_code()
                                == Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING),
                        "unexpected live mkfs error: {err:?}"
                    );
                }
            }
            let _ = fs::remove_dir_all(&mkfs_stage);
        }

        if unikernel_pack::tools::pack_tool_ready(PackMethod::DockerExport) {
            exercised = true;
            let docker_stage = temp_dir("live-docker");
            // Prefer a tiny public image; missing image → PACK_IO (fail-loud).
            let image = std::env::var("EIDOLON_PACK_DOCKER_IMAGE")
                .unwrap_or_else(|_| "alpine:3.19".to_string());
            match unikernel_pack::live_pack(
                &PackRequest::hermetic(&image, RootfsFormat::Raw)
                    .with_method(PackMethod::DockerExport)
                    .with_staging(&docker_stage, false),
            ) {
                Ok(m) => {
                    assert_eq!(m.pack_method, PackMethod::DockerExport);
                    assert_eq!(m.rootfs_format, "raw", "tar must stay Raw");
                    assert!(m.rootfs.is_file());
                    let _ = m.to_rootfs_config().expect("tar rootfs path");
                }
                Err(err) => {
                    assert!(
                        err.to_string().contains(codes::SANDBOX_ROOTFS_PACK_IO)
                            || err.unsupported_code()
                                == Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING),
                        "unexpected live docker export error: {err:?}"
                    );
                }
            }
            let _ = fs::remove_dir_all(&docker_stage);
        }

        if unikernel_pack::tools::pack_tool_ready(PackMethod::DockerToExt4) {
            exercised = true;
            let compose_stage = temp_dir("live-compose");
            let image = std::env::var("EIDOLON_PACK_DOCKER_IMAGE")
                .unwrap_or_else(|_| "alpine:3.19".to_string());
            match unikernel_pack::live_pack(
                &PackRequest::hermetic(&image, RootfsFormat::Ext4)
                    .with_method(PackMethod::DockerToExt4)
                    .with_staging(&compose_stage, false)
                    .with_image_size(64 * 1024 * 1024),
            ) {
                Ok(m) => {
                    assert_eq!(m.pack_method, PackMethod::DockerToExt4);
                    assert_eq!(m.rootfs_format, "ext4");
                    assert!(m.rootfs.is_file());
                    let cfg = m.to_rootfs_config().expect("LaunchPlan-ready Ext4 disk");
                    assert_eq!(cfg.format, RootfsFormat::Ext4);
                }
                Err(err) => {
                    assert!(
                        err.to_string().contains(codes::SANDBOX_ROOTFS_PACK_IO)
                            || err.unsupported_code()
                                == Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING),
                        "unexpected live docker_to_ext4 error: {err:?}"
                    );
                }
            }
            let _ = fs::remove_dir_all(&compose_stage);
        }

        if !exercised {
            // Env set but no tools: still fail-loud on a live call.
            let err = unikernel_pack::live_pack(
                &PackRequest::hermetic(&tree, RootfsFormat::Ext4)
                    .with_method(PackMethod::VirtMakeFs)
                    .with_staging(&staging, false),
            )
            .unwrap_err();
            assert_eq!(
                err.unsupported_code(),
                Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING)
            );
        }

        let _ = fs::remove_dir_all(&staging);
        let _ = fs::remove_dir_all(&tree);
    }

    #[test]
    fn docker_export_ext4_refused_in_integration_suite() {
        let err = PackRequest::hermetic("alpine:3.19", RootfsFormat::Ext4)
            .with_method(PackMethod::DockerExport)
            .validate()
            .unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn compose_missing_mkfs_fail_loud() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var(PACK_INTEGRATION_ENV, "1");
        // Provide a fake docker override that exists as a file, but poison mkfs.
        let staging = temp_dir("compose-missing-mkfs");
        let fake_docker = staging.join("fake-docker-bin");
        {
            let mut f = fs::File::create(&fake_docker).unwrap();
            writeln!(f, "#!/bin/sh\nexit 1").unwrap();
            let mut perms = fs::metadata(&fake_docker).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&fake_docker, perms).unwrap();
        }
        std::env::set_var(DOCKER_PATH_ENV, &fake_docker);
        std::env::set_var(MKFS_PATH_ENV, "/no/such/eidolon-mkfs-compose");
        std::env::set_var(VIRT_MAKE_FS_PATH_ENV, "/no/such/eidolon-virt-compose");

        let err = unikernel_pack::compose_docker_to_ext4(
            &PackRequest::hermetic("alpine:3.19", RootfsFormat::Ext4)
                .with_method(PackMethod::DockerToExt4)
                .with_staging(&staging, false),
        )
        .unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING),
            "must fail-loud when mkfs missing even if docker resolves"
        );

        std::env::remove_var(PACK_INTEGRATION_ENV);
        std::env::remove_var(DOCKER_PATH_ENV);
        std::env::remove_var(MKFS_PATH_ENV);
        std::env::remove_var(VIRT_MAKE_FS_PATH_ENV);
        let _ = fs::remove_dir_all(&staging);
    }

    #[test]
    fn canned_pack_ext4_live_when_mkfs_ready() {
        let _g = ENV_LOCK.lock().unwrap();
        if std::env::var(PACK_INTEGRATION_ENV).ok().as_deref() != Some("1") {
            return;
        }
        if !unikernel_pack::tools::pack_tool_ready(PackMethod::MkfsExt4) {
            return;
        }

        let out = durable_temp_dir("canned-live-ext4");
        let host = durable_temp_dir("canned-live-host");
        let agent = fake_agent_bin(&host);

        match build_canned_rootfs(
            &CannedRootfsRequest::new()
                .with_out_dir(&out)
                .with_agent_bin(&agent)
                .with_mode(CannedMode::PackExt4)
                .with_image_size(8 * 1024 * 1024),
        ) {
            Ok(result) => {
                let img = result.rootfs_img.expect("ext4 img");
                assert!(img.is_file());
                assert_eq!(result.manifest.rootfs_format, "ext4");
            }
            Err(e) => {
                // Live tools can still fail (permissions, etc.) — never silent Ok without img.
                assert!(
                    e.unsupported_code() == Some(codes::SANDBOX_ROOTFS_PACK_IO)
                        || e.unsupported_code() == Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING)
                        || matches!(e, PhenoError::Internal(_)),
                    "unexpected live canned failure: {e:?}"
                );
            }
        }

        let _ = fs::remove_dir_all(&out);
        let _ = fs::remove_dir_all(&host);
    }
}

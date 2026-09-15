//! Cross-platform enforcement tests: policy translation + fail-loud apply.
//!
//! These run on **macOS CI/local** without Landlock/cgroup/namespaces/seccomp.
//! Live Linux apply is gated in `enforcement_linux.rs` behind
//! `LANDLOCK_INTEGRATION=1` / `CGROUP_INTEGRATION=1` /
//! `NAMESPACES_INTEGRATION=1` / `SECCOMP_INTEGRATION=1` (ignored by default).

use std::path::PathBuf;

use eidolon_core::error::PhenoError;
use eidolon_core::security::{NetworkPolicy, SandboxPolicy};
use eidolon_sandbox::codes;
use eidolon_sandbox::enforcement::{
    apply_cgroup, apply_isolation, apply_landlock, apply_namespaces, apply_seccomp, cgroup_ready,
    landlock_ready, namespaces_ready, plan_from_policy, resolve_pivot_rootfs, run_in_namespaces,
    seccomp_ready, LandlockFsRules, NamespacePlan, SeccompProfile, CGROUP_CPU_PERIOD_US,
    NS_PID_ENV, NS_PIVOT_ENV, NS_PIVOT_ROOTFS_ENV,
};

fn assert_code(err: PhenoError, expected: &str) {
    assert_eq!(err.unsupported_code(), Some(expected));
    assert_eq!(err.status_code(), 501);
}

#[test]
fn plan_translates_cpu_memory_net_namespaces_seccomp() {
    std::env::remove_var("EIDOLON_SECCOMP_PROFILE");
    let policy = SandboxPolicy {
        cpu_cores: 4,
        memory_mib: 1024,
        disk_mib: Some(2048),
        network: NetworkPolicy::Deny,
    };
    let fs = LandlockFsRules {
        read_only: vec![PathBuf::from("/usr")],
        read_write: vec![PathBuf::from("/tmp/eidolon")],
        execute: vec![PathBuf::from("/bin")],
    };
    let plan = plan_from_policy(&policy, fs).expect("plan");
    assert_eq!(plan.cgroup.memory_max_bytes, 1024 * 1024 * 1024);
    assert_eq!(plan.cgroup.cpu_quota_us, 4 * CGROUP_CPU_PERIOD_US);
    assert!(!plan.cgroup.disk_enforced);
    assert_eq!(plan.cgroup.disk_mib, Some(2048));
    assert!(plan.landlock_net_deny);
    assert_eq!(plan.landlock_fs.path_count(), 3);
    assert!(plan.namespaces.net);
    assert_eq!(plan.seccomp.profile, SeccompProfile::BlockDangerous);
}

#[test]
fn seccomp_profile_env_oci_default_in_plan() {
    std::env::set_var("EIDOLON_SECCOMP_PROFILE", "oci-default");
    let plan = plan_from_policy(&SandboxPolicy::default(), LandlockFsRules::deny_all())
        .expect("plan with oci-default");
    std::env::remove_var("EIDOLON_SECCOMP_PROFILE");
    assert_eq!(plan.seccomp.profile, SeccompProfile::OciDefault);
}

#[test]
fn seccomp_profile_env_invalid_fails_loud() {
    std::env::set_var("EIDOLON_SECCOMP_PROFILE", "not-a-valid-profile");
    let err = plan_from_policy(&SandboxPolicy::default(), LandlockFsRules::deny_all()).unwrap_err();
    std::env::remove_var("EIDOLON_SECCOMP_PROFILE");
    assert!(matches!(err, PhenoError::BadRequest(_)));
}

#[test]
fn probes_are_boolean_everywhere() {
    let _ = landlock_ready();
    let _ = cgroup_ready();
    let _ = namespaces_ready();
    let _ = seccomp_ready();
}

/// Off-platform / feature-off: apply must fail loud (never Ok).
#[test]
fn landlock_apply_fail_loud_off_platform_or_feature_off() {
    let plan = plan_from_policy(&SandboxPolicy::default(), LandlockFsRules::deny_all()).unwrap();
    #[cfg(not(all(feature = "sandbox-landlock", target_os = "linux")))]
    {
        let err = apply_landlock(&plan).unwrap_err();
        assert_code(err, codes::SANDBOX_LANDLOCK_UNSUPPORTED);
        assert!(!landlock_ready());
    }
    #[cfg(all(feature = "sandbox-landlock", target_os = "linux"))]
    {
        // Do **not** call apply_landlock here: a successful deny-all would
        // brick the test process. Coverage lives in enforcement_linux.rs.
        let _ = plan;
        let _ = landlock_ready();
    }
}

#[test]
fn cgroup_apply_fail_loud_off_platform_or_feature_off() {
    let plan = plan_from_policy(&SandboxPolicy::default(), LandlockFsRules::deny_all()).unwrap();
    #[cfg(not(all(feature = "sandbox-cgroup", target_os = "linux")))]
    {
        let err = apply_cgroup("sandbox-1", &plan).unwrap_err();
        assert_code(err, codes::SANDBOX_CGROUP_UNSUPPORTED);
        assert!(!cgroup_ready());
    }
    #[cfg(all(feature = "sandbox-cgroup", target_os = "linux"))]
    {
        let _ = plan;
        let _ = cgroup_ready();
    }
}

#[test]
fn namespaces_apply_fail_loud_off_platform_or_feature_off() {
    let plan = plan_from_policy(&SandboxPolicy::default(), LandlockFsRules::deny_all()).unwrap();
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        let err = apply_namespaces(&plan).unwrap_err();
        assert_code(err, codes::SANDBOX_NAMESPACES_UNSUPPORTED);
        assert!(!namespaces_ready());
    }
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        // Live unshare may succeed; covered by NAMESPACES_INTEGRATION.
        let _ = plan;
        let _ = namespaces_ready();
    }
}

#[test]
fn seccomp_apply_fail_loud_off_platform_or_feature_off() {
    let plan = plan_from_policy(&SandboxPolicy::default(), LandlockFsRules::deny_all()).unwrap();
    #[cfg(not(all(
        feature = "sandbox-seccomp",
        target_os = "linux",
        target_endian = "little"
    )))]
    {
        let err = apply_seccomp(&plan).unwrap_err();
        assert_code(err, codes::SANDBOX_SECCOMP_UNSUPPORTED);
        assert!(!seccomp_ready());
    }
    #[cfg(all(
        feature = "sandbox-seccomp",
        target_os = "linux",
        target_endian = "little"
    ))]
    {
        // Live apply installs an irreversible filter; covered by SECCOMP_INTEGRATION.
        let _ = plan;
        let _ = seccomp_ready();
    }
}

#[test]
fn apply_isolation_fail_loud_when_no_live_platform() {
    #[cfg(not(any(
        feature = "sandbox-landlock",
        feature = "sandbox-cgroup",
        feature = "sandbox-namespaces",
        feature = "sandbox-seccomp"
    )))]
    {
        let err = apply_isolation(
            "iso-1",
            &SandboxPolicy::default(),
            LandlockFsRules::deny_all(),
        )
        .unwrap_err();
        assert_code(err, codes::SANDBOX_ENFORCEMENT_DISABLED);
    }

    #[cfg(all(
        any(
            feature = "sandbox-landlock",
            feature = "sandbox-cgroup",
            feature = "sandbox-namespaces",
            feature = "sandbox-seccomp"
        ),
        not(target_os = "linux")
    ))]
    {
        let err = apply_isolation(
            "iso-1",
            &SandboxPolicy::default(),
            LandlockFsRules::deny_all(),
        )
        .unwrap_err();
        assert_eq!(err.status_code(), 501);
        let code = err.unsupported_code().unwrap();
        assert!(
            code == codes::SANDBOX_LANDLOCK_UNSUPPORTED
                || code == codes::SANDBOX_CGROUP_UNSUPPORTED
                || code == codes::SANDBOX_NAMESPACES_UNSUPPORTED
                || code == codes::SANDBOX_SECCOMP_UNSUPPORTED,
            "got {code}"
        );
    }
}

#[test]
fn cgroup_rejects_bad_sandbox_id_before_platform_check() {
    let plan = plan_from_policy(&SandboxPolicy::default(), LandlockFsRules::deny_all()).unwrap();
    let err = apply_cgroup("", &plan).unwrap_err();
    assert!(matches!(err, PhenoError::BadRequest(_)));
}

#[test]
fn pid_ns_env_constant_stable() {
    assert_eq!(NS_PID_ENV, "EIDOLON_SANDBOX_NS_PID");
}

#[test]
fn pivot_root_env_constants_stable() {
    assert_eq!(NS_PIVOT_ENV, "EIDOLON_SANDBOX_NS_PIVOT");
    assert_eq!(NS_PIVOT_ROOTFS_ENV, "EIDOLON_SANDBOX_NS_PIVOT_ROOTFS");
}

#[test]
fn resolve_pivot_rootfs_hermetic() {
    assert!(resolve_pivot_rootfs(false, Some("/x")).unwrap().is_none());
    let err = resolve_pivot_rootfs(true, None).unwrap_err();
    assert!(matches!(err, PhenoError::BadRequest(_)));
    let p = resolve_pivot_rootfs(true, Some("/tmp/rootfs"))
        .unwrap()
        .unwrap();
    assert_eq!(p.as_os_str(), "/tmp/rootfs");
}

#[test]
fn pid_ns_run_in_namespaces_fail_loud_off_platform() {
    let ns = NamespacePlan {
        uts: true,
        ipc: false,
        cgroup: false,
        net: false,
        mount: false,
        pid: true,
        user: false,
        uid_maps: Vec::new(),
        gid_maps: Vec::new(),
        pivot_rootfs: None,
    };
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        let err = run_in_namespaces(&ns, || Ok(())).unwrap_err();
        assert_code(err, codes::SANDBOX_NAMESPACES_UNSUPPORTED);
    }
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        // Live fork covered by NAMESPACES_INTEGRATION; do not unshare here.
        let _ = ns;
    }
}

#[test]
fn pivot_root_run_in_namespaces_fail_loud_off_platform() {
    let ns = NamespacePlan {
        uts: false,
        ipc: false,
        cgroup: false,
        net: false,
        mount: true,
        pid: false,
        user: false,
        uid_maps: Vec::new(),
        gid_maps: Vec::new(),
        pivot_rootfs: Some(std::path::PathBuf::from("/tmp/eidolon-pivot-rootfs")),
    };
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        let err = run_in_namespaces(&ns, || Ok(())).unwrap_err();
        assert_code(err, codes::SANDBOX_NAMESPACES_UNSUPPORTED);
    }
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        let _ = ns;
    }
}

#[test]
fn documented_codes_are_stable_strings() {
    assert_eq!(
        codes::SANDBOX_LANDLOCK_UNSUPPORTED,
        "EIDOLON_SANDBOX_LANDLOCK_UNSUPPORTED"
    );
    assert_eq!(
        codes::SANDBOX_CGROUP_UNSUPPORTED,
        "EIDOLON_SANDBOX_CGROUP_UNSUPPORTED"
    );
    assert_eq!(
        codes::SANDBOX_CGROUP_DISK_UNAVAILABLE,
        "EIDOLON_SANDBOX_CGROUP_DISK_UNAVAILABLE"
    );
    assert_eq!(
        codes::SANDBOX_NAMESPACES_UNSUPPORTED,
        "EIDOLON_SANDBOX_NAMESPACES_UNSUPPORTED"
    );
    assert_eq!(
        codes::SANDBOX_SECCOMP_UNSUPPORTED,
        "EIDOLON_SANDBOX_SECCOMP_UNSUPPORTED"
    );
    assert_eq!(
        codes::SANDBOX_ENFORCEMENT_DISABLED,
        "EIDOLON_SANDBOX_ENFORCEMENT_DISABLED"
    );
}

//! Linux-only live Landlock / cgroup / namespaces / seccomp integration (env-gated).
//!
//! ```text
//! LANDLOCK_INTEGRATION=1 cargo test -p eidolon-sandbox \
//!   --features sandbox-landlock --test enforcement_linux -- --ignored
//! CGROUP_INTEGRATION=1 cargo test -p eidolon-sandbox \
//!   --features sandbox-cgroup --test enforcement_linux -- --ignored
//! NAMESPACES_INTEGRATION=1 cargo test -p eidolon-sandbox \
//!   --features sandbox-namespaces --test enforcement_linux -- --ignored
//! NAMESPACES_INTEGRATION=1 EIDOLON_SANDBOX_NS_USER=1 cargo test -p eidolon-sandbox \
//!   --features sandbox-namespaces --test enforcement_linux user_ns -- --ignored
//! SECCOMP_INTEGRATION=1 cargo test -p eidolon-sandbox \
//!   --features sandbox-seccomp --test enforcement_linux -- --ignored
//! ```
//!
//! On macOS these tests compile as no-ops / skips so the file stays in the
//! default test set without breaking CI. Off-Linux with integration env set
//! panics (fail-loud). USER ns multi-range needs `newuidmap`/`newgidmap` or
//! `/etc/subuid` delegation — see `docs/guides/linux-live-smokes.md`.

#![allow(unused_imports)]

use eidolon_core::security::{NetworkPolicy, SandboxPolicy};
use eidolon_sandbox::enforcement::{
    apply_cgroup, apply_landlock, apply_namespaces, apply_seccomp, env_requests_user_ns,
    plan_from_policy, LandlockFsRules, NamespacePlan, UserNsIdMap, NS_USER_ENV,
};

fn env_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
    )
}

fn user_ns_smoke_env_ready() -> bool {
    env_truthy("NAMESPACES_INTEGRATION") && env_truthy(NS_USER_ENV)
}

#[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
fn proc_id_field(prefix: &str) -> u32 {
    let status = std::fs::read_to_string("/proc/self/status").expect("status");
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix(prefix) {
            return rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| panic!("parse {prefix} from {line}"));
        }
    }
    panic!("missing {prefix} in /proc/self/status");
}

#[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
fn idmap_tool_on_path(name: &str) -> bool {
    let Ok(path_var) = std::env::var("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return true;
        }
    }
    [format!("/usr/bin/{name}"), format!("/bin/{name}")]
        .into_iter()
        .map(std::path::PathBuf::from)
        .any(|p| p.is_file())
}

#[test]
#[ignore = "set LANDLOCK_INTEGRATION=1 on Linux + --features sandbox-landlock"]
fn landlock_live_restrict_self() {
    if !env_truthy("LANDLOCK_INTEGRATION") {
        eprintln!("skip: LANDLOCK_INTEGRATION not set");
        return;
    }
    #[cfg(not(all(feature = "sandbox-landlock", target_os = "linux")))]
    {
        panic!("LANDLOCK_INTEGRATION requires Linux + feature sandbox-landlock");
    }
    #[cfg(all(feature = "sandbox-landlock", target_os = "linux"))]
    {
        // Use a temporary writable dir so restrict_self has at least one grant.
        let tmp = std::env::temp_dir().join("eidolon-landlock-it");
        let _ = std::fs::create_dir_all(&tmp);
        let fs = LandlockFsRules {
            read_only: vec![],
            read_write: vec![tmp],
            execute: vec![],
        };
        let plan = plan_from_policy(&SandboxPolicy::default(), fs).expect("plan");
        let status = apply_landlock(&plan).expect("landlock apply");
        assert_eq!(status.abi, "V1");
        assert_eq!(status.rules_installed, 1);
        // Network deny is best-effort; do not assert net_handled.
    }
}

#[test]
#[ignore = "set CGROUP_INTEGRATION=1 on Linux + --features sandbox-cgroup"]
fn cgroup_live_memory_cpu() {
    if !env_truthy("CGROUP_INTEGRATION") {
        eprintln!("skip: CGROUP_INTEGRATION not set");
        return;
    }
    #[cfg(not(all(feature = "sandbox-cgroup", target_os = "linux")))]
    {
        panic!("CGROUP_INTEGRATION requires Linux + feature sandbox-cgroup");
    }
    #[cfg(all(feature = "sandbox-cgroup", target_os = "linux"))]
    {
        // Default policy sets disk_mib; live memory/CPU apply without I/O needs
        // disk_mib: None (or EIDOLON_CGROUP_DISK_DEV for io.max).
        let plan = plan_from_policy(
            &SandboxPolicy {
                disk_mib: None,
                ..SandboxPolicy::default()
            },
            LandlockFsRules::deny_all(),
        )
        .expect("plan");
        let status = apply_cgroup("itest1", &plan).expect("cgroup apply");
        assert!(status.cgroup_path.contains("eidolon-itest1"));
        assert_eq!(status.memory_max_bytes, 512 * 1024 * 1024);
        assert!(!status.disk_enforced);
        assert!(status.io_max.is_none());
    }
}

#[test]
#[ignore = "set NAMESPACES_INTEGRATION=1 on Linux + --features sandbox-namespaces"]
fn namespaces_live_unshare_uts() {
    if !env_truthy("NAMESPACES_INTEGRATION") {
        eprintln!("skip: NAMESPACES_INTEGRATION not set");
        return;
    }
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        panic!("NAMESPACES_INTEGRATION requires Linux + feature sandbox-namespaces");
    }
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        // UTS-only: least disruptive; avoid NEWNET (breaks suite networking).
        let mut plan = plan_from_policy(
            &SandboxPolicy {
                network: NetworkPolicy::Allow,
                ..SandboxPolicy::default()
            },
            LandlockFsRules::deny_all(),
        )
        .expect("plan");
        plan.namespaces = NamespacePlan {
            uts: true,
            ipc: false,
            cgroup: false,
            net: false,
            mount: false,
            pid: false,
            user: false,
            uid_maps: Vec::new(),
            gid_maps: Vec::new(),
            pivot_rootfs: None,
        };
        let status = apply_namespaces(&plan).expect("namespaces apply");
        assert_eq!(status.status.flags_applied, vec!["uts"]);
        assert!(!status.status.pid_ns_isolates_caller);
        assert!(status.status.isolated_child_host_pid.is_none());
        assert!(!status.status.pivot_root_applied);
        assert!(status.setup.is_none());
    }
}

#[test]
#[ignore = "set NAMESPACES_INTEGRATION=1 on Linux + --features sandbox-namespaces"]
fn namespaces_live_pid_ns_fork_child_is_pid1() {
    if !env_truthy("NAMESPACES_INTEGRATION") {
        eprintln!("skip: NAMESPACES_INTEGRATION not set");
        return;
    }
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        panic!("NAMESPACES_INTEGRATION requires Linux + feature sandbox-namespaces");
    }
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        use eidolon_sandbox::enforcement::run_in_namespaces;
        // UTS+PID only — avoid NEWNET. Child must see getpid()==1.
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
        let status = run_in_namespaces(&ns, || Ok(())).expect("PID-ns run_in_namespaces");
        assert!(status.flags_applied.contains(&"pid"));
        assert!(status.pid_ns_isolates_caller);
        assert!(status.isolated_child_host_pid.is_some());
        assert!(!status.pivot_root_applied);
    }
}

#[test]
#[ignore = "set NAMESPACES_INTEGRATION=1 on Linux + --features sandbox-namespaces + CAP_SYS_ADMIN"]
fn namespaces_live_pivot_root_in_pid_child() {
    if !env_truthy("NAMESPACES_INTEGRATION") {
        eprintln!("skip: NAMESPACES_INTEGRATION not set");
        return;
    }
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        panic!("NAMESPACES_INTEGRATION requires Linux + feature sandbox-namespaces");
    }
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        use eidolon_sandbox::enforcement::run_in_namespaces;
        use std::fs;
        use std::path::PathBuf;

        // Minimal rootfs dir; bind+pivot happens inside the PID-ns child so the
        // test process keeps the host root.
        let root =
            std::env::temp_dir().join(format!("eidolon-pivot-rootfs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create rootfs");
        fs::write(root.join("eidolon-pivot-marker"), b"ok").expect("marker");

        let ns = NamespacePlan {
            uts: true,
            ipc: false,
            cgroup: false,
            net: false,
            mount: true,
            pid: true,
            user: false,
            uid_maps: Vec::new(),
            gid_maps: Vec::new(),
            pivot_rootfs: Some(root.clone()),
        };
        let status = run_in_namespaces(&ns, || {
            let marker = PathBuf::from("/eidolon-pivot-marker");
            assert!(
                marker.is_file(),
                "after pivot_root, marker must be at /eidolon-pivot-marker"
            );
            Ok(())
        })
        .expect("pivot_root run_in_namespaces");
        assert!(status.flags_applied.contains(&"mount"));
        assert!(status.flags_applied.contains(&"pid"));
        assert!(status.pid_ns_isolates_caller);
        assert!(status.pivot_root_applied);
        let _ = fs::remove_dir_all(&root);
    }
}

#[test]
#[ignore = "set NAMESPACES_INTEGRATION=1 on Linux + --features sandbox-namespaces"]
fn namespaces_live_user_ns_identity_map() {
    if !env_truthy("NAMESPACES_INTEGRATION") {
        eprintln!("skip: NAMESPACES_INTEGRATION not set");
        return;
    }
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        panic!("NAMESPACES_INTEGRATION requires Linux + feature sandbox-namespaces");
    }
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        use eidolon_sandbox::enforcement::{run_in_namespaces, UserNsIdMap};

        let euid = proc_id_field("Uid:\t");
        let egid = proc_id_field("Gid:\t");
        let ns = NamespacePlan {
            uts: true,
            ipc: false,
            cgroup: false,
            net: false,
            mount: false,
            pid: false,
            user: true,
            uid_maps: vec![UserNsIdMap {
                inside: 0,
                outside: euid,
                count: 1,
            }],
            gid_maps: vec![UserNsIdMap {
                inside: 0,
                outside: egid,
                count: 1,
            }],
            pivot_rootfs: None,
        };
        let status = run_in_namespaces(&ns, || {
            let inside_uid = proc_id_field("Uid:\t");
            assert_eq!(
                inside_uid, 0,
                "identity USER ns map should be root inside the new USER ns"
            );
            Ok(())
        })
        .expect("USER ns run_in_namespaces");
        assert!(status.flags_applied.contains(&"user"));
        assert!(status.user_ns_mapped);
    }
}

#[test]
#[ignore = "set NAMESPACES_INTEGRATION=1 + EIDOLON_SANDBOX_NS_USER=1 on Linux + --features sandbox-namespaces"]
fn namespaces_live_user_ns_env_gate_identity() {
    if !user_ns_smoke_env_ready() {
        eprintln!("skip: need NAMESPACES_INTEGRATION=1 and {NS_USER_ENV}=1 for USER ns smoke");
        return;
    }
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        panic!(
            "NAMESPACES_INTEGRATION + {NS_USER_ENV} requires Linux + feature sandbox-namespaces"
        );
    }
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        use eidolon_sandbox::enforcement::run_in_namespaces;

        assert!(
            env_requests_user_ns(),
            "{NS_USER_ENV} must be visible to plan_from_policy in this smoke"
        );
        let plan = plan_from_policy(
            &SandboxPolicy {
                network: NetworkPolicy::Allow,
                ..SandboxPolicy::default()
            },
            LandlockFsRules::deny_all(),
        )
        .expect("plan_from_policy with USER ns env");
        assert!(plan.namespaces.user);
        assert_eq!(plan.namespaces.uid_maps.len(), 1);
        assert_eq!(plan.namespaces.gid_maps.len(), 1);

        let mut ns = plan.namespaces.clone();
        // UTS-only unshare alongside USER maps — avoid NEWNET for suite stability.
        ns.uts = true;
        ns.ipc = false;
        ns.cgroup = false;
        ns.net = false;
        ns.mount = false;
        ns.pid = false;

        let status = run_in_namespaces(&ns, || {
            let inside_uid = proc_id_field("Uid:\t");
            assert_eq!(
                inside_uid, 0,
                "env-gated USER ns map should be root inside the new USER ns"
            );
            Ok(())
        })
        .expect("USER ns env-gate run_in_namespaces");
        assert!(status.flags_applied.contains(&"user"));
        assert!(status.user_ns_mapped);
    }
}

#[test]
#[ignore = "set NAMESPACES_INTEGRATION=1 + EIDOLON_SANDBOX_NS_USER=1 on Linux + newuidmap when multi-range"]
fn namespaces_live_user_ns_multi_range_or_fail_loud() {
    if !user_ns_smoke_env_ready() {
        eprintln!("skip: need NAMESPACES_INTEGRATION=1 and {NS_USER_ENV}=1 for USER ns smoke");
        return;
    }
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        panic!(
            "NAMESPACES_INTEGRATION + {NS_USER_ENV} requires Linux + feature sandbox-namespaces"
        );
    }
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        use eidolon_core::error::PhenoError;
        use eidolon_sandbox::codes;
        use eidolon_sandbox::enforcement::run_in_namespaces;

        if !(idmap_tool_on_path("newuidmap") && idmap_tool_on_path("newgidmap")) {
            panic!(
                "USER ns multi-range smoke requires newuidmap and newgidmap on PATH \
                 (install shadow-utils uidmap); refusing silent skip on Linux"
            );
        }

        let euid = proc_id_field("Uid:\t");
        let egid = proc_id_field("Gid:\t");
        // count > 1 forces the newuidmap/newgidmap path (not direct /proc write).
        let ns = NamespacePlan {
            uts: true,
            ipc: false,
            cgroup: false,
            net: false,
            mount: false,
            pid: false,
            user: true,
            uid_maps: vec![UserNsIdMap {
                inside: 0,
                outside: euid,
                count: 65536,
            }],
            gid_maps: vec![UserNsIdMap {
                inside: 0,
                outside: egid,
                count: 65536,
            }],
            pivot_rootfs: None,
        };

        match run_in_namespaces(&ns, || Ok(())) {
            Ok(status) => {
                assert!(status.flags_applied.contains(&"user"));
                assert!(
                    status.user_ns_mapped,
                    "multi-range USER ns must report user_ns_mapped on success"
                );
            }
            Err(err) => {
                let code = match &err {
                    PhenoError::UnsupportedPlatform { code, .. } => code.as_str(),
                    other => panic!("expected UnsupportedPlatform fail-loud, got {other:?}"),
                };
                assert_eq!(
                    code,
                    codes::SANDBOX_NAMESPACES_UNSUPPORTED,
                    "multi-range miss must fail loud with namespaces unsupported, not silent skip"
                );
            }
        }
    }
}

#[test]
#[ignore = "set SECCOMP_INTEGRATION=1 on Linux + --features sandbox-seccomp"]
fn seccomp_live_block_dangerous() {
    if !env_truthy("SECCOMP_INTEGRATION") {
        eprintln!("skip: SECCOMP_INTEGRATION not set");
        return;
    }
    #[cfg(not(all(
        feature = "sandbox-seccomp",
        target_os = "linux",
        target_endian = "little"
    )))]
    {
        panic!("SECCOMP_INTEGRATION requires Linux LE + feature sandbox-seccomp");
    }
    #[cfg(all(
        feature = "sandbox-seccomp",
        target_os = "linux",
        target_endian = "little"
    ))]
    {
        let plan =
            plan_from_policy(&SandboxPolicy::default(), LandlockFsRules::deny_all()).expect("plan");
        let status = apply_seccomp(&plan).expect("seccomp apply");
        assert_eq!(status.profile, "block-dangerous");
        assert!(status.blocked_syscalls >= 8);
        assert_eq!(status.allowed_syscalls, 0);
        assert!(status.no_new_privs);
    }
}

#[test]
#[ignore = "set SECCOMP_INTEGRATION=1 on Linux + --features sandbox-seccomp"]
fn seccomp_live_oci_default() {
    if !env_truthy("SECCOMP_INTEGRATION") {
        eprintln!("skip: SECCOMP_INTEGRATION not set");
        return;
    }
    #[cfg(not(all(
        feature = "sandbox-seccomp",
        target_os = "linux",
        target_endian = "little"
    )))]
    {
        panic!("SECCOMP_INTEGRATION requires Linux LE + feature sandbox-seccomp");
    }
    #[cfg(all(
        feature = "sandbox-seccomp",
        target_os = "linux",
        target_endian = "little"
    ))]
    {
        // Avoid mutating process env for other ignored tests in the same binary:
        // build plan then override profile.
        let mut plan =
            plan_from_policy(&SandboxPolicy::default(), LandlockFsRules::deny_all()).expect("plan");
        plan.seccomp.profile = eidolon_sandbox::enforcement::SeccompProfile::OciDefault;
        let status = apply_seccomp(&plan).expect("oci-default seccomp apply");
        assert_eq!(status.profile, "oci-default");
        assert_eq!(status.blocked_syscalls, 0);
        assert!(status.allowed_syscalls >= 16);
        assert!(status.no_new_privs);
    }
}

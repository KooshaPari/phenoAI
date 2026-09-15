//! Live Linux isolation enforcement hooks (Landlock + cgroup v2 + namespaces + seccomp).
//!
//! # Honesty
//!
//! | Surface | When live | Otherwise |
//! |---|---|---|
//! | [`apply_landlock`] | Linux + feature `sandbox-landlock` + kernel ABI ≥ V1 | [`codes::SANDBOX_LANDLOCK_UNSUPPORTED`](crate::codes::SANDBOX_LANDLOCK_UNSUPPORTED) |
//! | [`apply_cgroup`] | Linux + feature `sandbox-cgroup` + writable cgroup v2 (+ `io.max` when `disk_mib` set) | [`codes::SANDBOX_CGROUP_UNSUPPORTED`](crate::codes::SANDBOX_CGROUP_UNSUPPORTED) / [`codes::SANDBOX_CGROUP_DISK_UNAVAILABLE`](crate::codes::SANDBOX_CGROUP_DISK_UNAVAILABLE) |
//! | [`apply_namespaces`] | Linux + feature `sandbox-namespaces` + capable unshare (+ fork when PID ns) | [`codes::SANDBOX_NAMESPACES_UNSUPPORTED`](crate::codes::SANDBOX_NAMESPACES_UNSUPPORTED) |
//! | [`apply_seccomp`] | Linux LE + feature `sandbox-seccomp` + filterable kernel | [`codes::SANDBOX_SECCOMP_UNSUPPORTED`](crate::codes::SANDBOX_SECCOMP_UNSUPPORTED) |
//! | [`plan_from_policy`] | Always (pure translation) | [`PhenoError::BadRequest`](eidolon_core::PhenoError) on zero ceilings |
//! | [`EnforcingSandbox`] / [`apply_enabled_enforcement`] | Composition-time wire on `start` | No features → [`codes::SANDBOX_ENFORCEMENT_DISABLED`](crate::codes::SANDBOX_ENFORCEMENT_DISABLED); off-platform → per-hook 501 codes |
//!
//! macOS / Windows / feature-off builds compile the same API and **fail loud** —
//! they never return `Ok` from apply helpers. Policy translation unit tests run
//! everywhere; optional Linux integration uses `LANDLOCK_INTEGRATION=1` /
//! `CGROUP_INTEGRATION=1` / `NAMESPACES_INTEGRATION=1` / `SECCOMP_INTEGRATION=1`.
//!
//! Disk ceilings on [`SandboxPolicy`](eidolon_core::security::SandboxPolicy):
//! when `disk_mib` is `Some`, cgroup apply writes **`io.max`** (bandwidth
//! stand-in; no portable capacity controller) or fails with
//! [`codes::SANDBOX_CGROUP_DISK_UNAVAILABLE`](crate::codes::SANDBOX_CGROUP_DISK_UNAVAILABLE).
//! PID ns (`NamespacePlan.pid` / `EIDOLON_SANDBOX_NS_PID=1`) **forks** after
//! `unshare(NEWPID)` so the child is PID 1; status
//! [`NamespaceStatus::pid_ns_isolates_caller`](crate::enforcement::NamespaceStatus::pid_ns_isolates_caller)
//! mirrors `disk_enforced` honesty. Parent awaits a setup-pipe ready byte
//! before claiming start success; child reports after later hooks or `_exit`s.
//! Mount `pivot_root` (`EIDOLON_SANDBOX_NS_PIVOT=1` +
//! `EIDOLON_SANDBOX_NS_PIVOT_ROOTFS`) runs after `CLONE_NEWNS`; status
//! [`NamespaceStatus::pivot_root_applied`](crate::enforcement::NamespaceStatus::pivot_root_applied).
//! Seccomp defaults to **block-dangerous**; opt into OCI allowlist via
//! `EIDOLON_SECCOMP_PROFILE=oci-default` or a JSON path.
//!
//! wraps: landlock 0.4 (optional, Linux-only) — Landlock LSM helpers  
//! wraps: nix 0.31 (optional, Linux-only) — `sched::unshare` + `unistd::fork` /
//!   `mount` + `pivot_root`  
//! wraps: seccompiler 0.5 (optional, Linux LE) — seccomp-bpf

mod cgroup;
mod landlock;
mod namespaces;
#[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
mod namespaces_pivot;
#[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
mod namespaces_user;
mod plan;
mod seccomp;
mod seccomp_oci;
mod seccomp_profile;
mod seccomp_syscall_nr;
mod wiring;

pub use cgroup::{
    apply as apply_cgroup, bytes_per_sec_from_disk_mib, cgroup_ready, io_max_line, parse_maj_min,
    resolve_disk_device, CgroupStatus, DISK_DEV_ENV, DISK_PATH_ENV,
};
pub use landlock::{apply as apply_landlock, landlock_ready, LandlockStatus};
pub use namespaces::{
    apply as apply_namespaces, env_requests_pid_ns, env_requests_pivot_root, env_requests_user_ns,
    env_requests_user_subids, env_user_ns_maps, namespaces_ready, parse_id_map_spec,
    parse_subid_file, park_until_signal, resolve_pivot_rootfs, run_in_namespaces,
    terminate_isolated_child, NamespaceApply, NamespaceStatus, PidNsChildSetup, PidNsParentSetup,
    PidNsSetup, NS_GID_MAP_ENV, NS_PID_ENV, NS_PIVOT_ENV, NS_PIVOT_ROOTFS_ENV, NS_UID_MAP_ENV,
    NS_USER_ENV, NS_USER_SUBIDS_ENV, SUBGID_FILE, SUBUID_FILE,
};
#[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
pub use namespaces_user::{NEWGIDMAP_ENV, NEWUIDMAP_ENV};
pub use plan::{
    plan_from_policy, CgroupPlan, EnforcementPlan, LandlockFsRules, NamespacePlan, SeccompPlan,
    SeccompProfile, UserNsIdMap, CGROUP_CPU_PERIOD_US,
};
pub use seccomp::{apply as apply_seccomp, seccomp_ready, SeccompStatus};
pub use seccomp_oci::{
    load_oci_allowlist_file, oci_default_allowlist, parse_oci_allowlist, to_seccompiler_allowlist_json,
    OCI_DEFAULT_JSON,
};
pub use seccomp_profile::{
    parse_seccomp_profile_spec, profile_label, resolve_seccomp_profile, SECCOMP_PROFILE_ENV,
};
pub use wiring::{apply_enabled_enforcement, AppliedEnforcement, EnforcingSandbox};

use eidolon_core::Result;
use eidolon_core::security::SandboxPolicy;

/// Combined apply for features compiled into this build.
///
/// Order: namespaces → cgroup → Landlock → seccomp. Namespaces first so
/// subsequent cgroup joins happen in the intended hierarchy; seccomp last so
/// setup syscalls are not blocked mid-apply. When PID ns is requested, the
/// parent awaits the child's setup-pipe ready byte (after remaining hooks);
/// the child continues those hooks, reports success and parks, or `_exit`s.
///
/// Each step runs only when its feature is enabled. With **no** enforcement
/// features this returns [`codes::SANDBOX_ENFORCEMENT_DISABLED`](crate::codes::SANDBOX_ENFORCEMENT_DISABLED).
pub fn apply_isolation(
    sandbox_id: &str,
    policy: &SandboxPolicy,
    fs: LandlockFsRules,
) -> Result<IsolationStatus> {
    let applied = apply_enabled_enforcement(sandbox_id, policy, fs)?;
    Ok(IsolationStatus {
        namespaces: applied.namespaces,
        cgroup: applied.cgroup,
        landlock: applied.landlock,
        seccomp: applied.seccomp,
    })
}

/// Status from [`apply_isolation`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolationStatus {
    pub namespaces: Option<NamespaceStatus>,
    pub cgroup: Option<CgroupStatus>,
    pub landlock: Option<LandlockStatus>,
    pub seccomp: Option<SeccompStatus>,
}

/// Probe helpers (always-on; return `false` off-platform / feature-off).
pub mod probe {
    pub use super::cgroup_ready;
    pub use super::landlock_ready;
    pub use super::namespaces_ready;
    pub use super::seccomp_ready;
}

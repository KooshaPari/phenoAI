//! Pure policy → enforcement plan translation.
//!
//! Compiles and runs on every platform so macOS CI can assert the mapping
//! without touching Landlock / cgroup / namespace / seccomp syscalls.

use eidolon_core::error::PhenoError;
use eidolon_core::security::{NetworkPolicy, SandboxPolicy};
use eidolon_core::Result;
use std::path::PathBuf;

/// Default `cpu.max` period (100 ms) used by cgroup v2 documentation.
pub const CGROUP_CPU_PERIOD_US: u64 = 100_000;

/// Filesystem path grants for a Landlock ruleset.
///
/// [`SandboxPolicy`] carries resource + network shape only; path allow-lists
/// are expressed here so policy remains a value type while Landlock gets the
/// concrete hierarchies it needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LandlockFsRules {
    /// Paths granted read-file + read-dir (no write / execute).
    pub read_only: Vec<PathBuf>,
    /// Paths granted read + write (no execute unless also listed in
    /// [`Self::execute`]).
    pub read_write: Vec<PathBuf>,
    /// Paths granted execute (typically also need read for ELF loaders).
    pub execute: Vec<PathBuf>,
}

impl LandlockFsRules {
    /// Empty ruleset — after `handle_access(all)`, every FS action is denied.
    pub fn deny_all() -> Self {
        Self::default()
    }

    /// Total path entries (for plan introspection / tests).
    pub fn path_count(&self) -> usize {
        self.read_only.len() + self.read_write.len() + self.execute.len()
    }
}

/// Planned cgroup v2 limits derived from [`SandboxPolicy`].
///
/// `disk_mib` is carried into the live [`super::apply_cgroup`] path. cgroup v2
/// has no portable capacity controller; when `disk_mib` is `Some`, apply writes
/// `io.max` rbps/wbps (MiB/s stand-in) for a known device, or fails loud with
/// [`crate::codes::SANDBOX_CGROUP_DISK_UNAVAILABLE`]. `disk_enforced` on the
/// plan stays `false` (claim comes from [`super::CgroupStatus`] after apply).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CgroupPlan {
    /// `memory.max` value in bytes.
    pub memory_max_bytes: u64,
    /// `cpu.max` quota (microseconds of CPU time per period).
    pub cpu_quota_us: u64,
    /// `cpu.max` period (microseconds). Always [`CGROUP_CPU_PERIOD_US`].
    pub cpu_period_us: u64,
    /// Requested disk ceiling from policy — applied as `io.max` bandwidth
    /// (not filesystem capacity) when apply succeeds.
    pub disk_mib: Option<u32>,
    /// Always `false` on the plan; see [`super::CgroupStatus::disk_enforced`].
    pub disk_enforced: bool,
}

/// Single-range USER namespace id map line (`inside outside count`).
///
/// Written to `/proc/self/uid_map` or `/proc/self/gid_map` after
/// `unshare(CLONE_NEWUSER)`. Default unprivileged shape is
/// `inside=0, outside=<host euid/egid>, count=1` (root inside the ns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserNsIdMap {
    pub inside: u32,
    pub outside: u32,
    pub count: u32,
}

impl UserNsIdMap {
    /// Kernel `/proc` map line body (no trailing newline).
    pub fn proc_line(&self) -> String {
        format!("{} {} {}", self.inside, self.outside, self.count)
    }
}

/// Which Linux namespaces to `unshare` for the current process.
///
/// Defaults from [`plan_from_policy`]: UTS + IPC + cgroup always; net when
/// network is Deny/EgressAllowList. Mount / user stay off unless callers
/// mutate the plan. PID is off by default; set via plan mutation or
/// `EIDOLON_SANDBOX_NS_PID=1` (see [`super::namespaces::env_requests_pid_ns`]).
/// When `pid` is true, apply **forks** so the child is PID 1 — see
/// [`super::namespaces`](crate::enforcement::namespaces).
///
/// Mount `pivot_root`: set [`Self::pivot_rootfs`] (implies [`Self::mount`])
/// via plan mutation or `EIDOLON_SANDBOX_NS_PIVOT=1` +
/// `EIDOLON_SANDBOX_NS_PIVOT_ROOTFS` (see
/// [`super::namespaces::env_requests_pivot_root`]).
///
/// USER ns maps: set [`Self::user`] + [`Self::uid_maps`] / [`Self::gid_maps`]
/// via plan mutation or `EIDOLON_SANDBOX_NS_USER=1` (+ optional
/// `EIDOLON_SANDBOX_NS_UID_MAP` / `EIDOLON_SANDBOX_NS_GID_MAP` or
/// `EIDOLON_SANDBOX_NS_USER_SUBIDS=1`; see
/// [`super::namespaces::env_requests_user_ns`]). Status honesty:
/// [`super::NamespaceStatus::user_ns_mapped`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespacePlan {
    pub uts: bool,
    pub ipc: bool,
    pub cgroup: bool,
    pub net: bool,
    pub mount: bool,
    pub pid: bool,
    pub user: bool,
    /// When `Some`, after mount-ns unshare perform `pivot_root` into this
    /// directory. Requires [`Self::mount`]. Status honesty:
    /// [`super::NamespaceStatus::pivot_root_applied`].
    pub pivot_rootfs: Option<PathBuf>,
    /// uid_map ranges when [`Self::user`] is true. Empty → apply uses
    /// `0:<euid>:1`. Multiple ranges or `count > 1` invoke `newuidmap`.
    pub uid_maps: Vec<UserNsIdMap>,
    /// gid_map ranges when [`Self::user`] is true. Empty → apply uses
    /// `0:<egid>:1`. Multiple ranges or `count > 1` invoke `newgidmap`.
    pub gid_maps: Vec<UserNsIdMap>,
}

impl NamespacePlan {
    /// Safe default for process-local unshare (no mount/pid/user/pivot).
    pub fn process_local(net: bool) -> Self {
        Self {
            uts: true,
            ipc: true,
            cgroup: true,
            net,
            mount: false,
            pid: false,
            user: false,
            pivot_rootfs: None,
            uid_maps: Vec::new(),
            gid_maps: Vec::new(),
        }
    }

    /// Names requested (for plan introspection / tests).
    pub fn requested_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.uts {
            names.push("uts");
        }
        if self.ipc {
            names.push("ipc");
        }
        if self.cgroup {
            names.push("cgroup");
        }
        if self.net {
            names.push("net");
        }
        if self.mount {
            names.push("mount");
        }
        if self.pid {
            names.push("pid");
        }
        if self.user {
            names.push("user");
        }
        if self.pivot_rootfs.is_some() {
            names.push("pivot_root");
        }
        names
    }
}

/// Seccomp profile selected for the enforcement plan.
///
/// Chosen via [`EIDOLON_SECCOMP_PROFILE`](super::seccomp_profile::SECCOMP_PROFILE_ENV)
/// (`block-dangerous` \| `oci-default` \| path to OCI JSON). Default remains
/// [`Self::BlockDangerous`] for back-compat; opt into allowlists explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeccompProfile {
    /// Allow unmatched syscalls; `EPERM` a fixed high-risk set (default).
    BlockDangerous,
    /// Embedded Docker/OCI-style unconditional syscall allowlist.
    OciDefault,
    /// Filesystem path to an OCI/Docker seccomp JSON profile.
    Custom(std::path::PathBuf),
}

/// Planned seccomp filter derived from [`SandboxPolicy`] + env profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeccompPlan {
    pub profile: SeccompProfile,
}

impl Default for SeccompPlan {
    fn default() -> Self {
        Self {
            profile: SeccompProfile::BlockDangerous,
        }
    }
}

/// Full isolation plan: cgroup + Landlock + namespaces + seccomp intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnforcementPlan {
    pub cgroup: CgroupPlan,
    pub landlock_fs: LandlockFsRules,
    pub network: NetworkPolicy,
    /// When `true`, Landlock ABI V4+ net rights should be handled if available
    /// (best-effort; older kernels skip net without pretending Deny holds).
    pub landlock_net_deny: bool,
    pub namespaces: NamespacePlan,
    pub seccomp: SeccompPlan,
}

/// Translate [`SandboxPolicy`] (+ optional FS rules) into an [`EnforcementPlan`].
///
/// Returns [`PhenoError::BadRequest`] when `cpu_cores` or `memory_mib` is zero
/// (a zero ceiling is not a meaningful isolation guarantee), or when
/// [`EIDOLON_SECCOMP_PROFILE`](super::seccomp_profile::SECCOMP_PROFILE_ENV) is set to an
/// invalid / unreadable / empty profile.
pub fn plan_from_policy(policy: &SandboxPolicy, fs: LandlockFsRules) -> Result<EnforcementPlan> {
    if policy.cpu_cores == 0 {
        return Err(PhenoError::BadRequest(
            "SandboxPolicy.cpu_cores must be >= 1 for enforcement".into(),
        ));
    }
    if policy.memory_mib == 0 {
        return Err(PhenoError::BadRequest(
            "SandboxPolicy.memory_mib must be >= 1 for enforcement".into(),
        ));
    }

    let landlock_net_deny = matches!(
        policy.network,
        NetworkPolicy::Deny | NetworkPolicy::EgressAllowList
    );

    let mut namespaces = NamespacePlan::process_local(landlock_net_deny);
    if crate::enforcement::namespaces::env_requests_pid_ns() {
        namespaces.pid = true;
    }
    if let Some(rootfs) = crate::enforcement::namespaces::env_pivot_rootfs_path()? {
        namespaces.mount = true;
        namespaces.pivot_rootfs = Some(rootfs);
    }
    if let Some((uid_maps, gid_maps)) = crate::enforcement::namespaces::env_user_ns_maps()? {
        namespaces.user = true;
        namespaces.uid_maps = uid_maps;
        namespaces.gid_maps = gid_maps;
    }

    Ok(EnforcementPlan {
        cgroup: CgroupPlan {
            memory_max_bytes: u64::from(policy.memory_mib).saturating_mul(1024 * 1024),
            cpu_quota_us: u64::from(policy.cpu_cores).saturating_mul(CGROUP_CPU_PERIOD_US),
            cpu_period_us: CGROUP_CPU_PERIOD_US,
            disk_mib: policy.disk_mib,
            disk_enforced: false,
        },
        landlock_fs: fs,
        network: policy.network,
        landlock_net_deny,
        namespaces,
        seccomp: SeccompPlan {
            profile: super::seccomp_profile::resolve_seccomp_profile()?,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use eidolon_core::security::NetworkPolicy;

    #[test]
    fn default_policy_maps_to_safe_cgroup_numbers() {
        std::env::remove_var(super::super::seccomp_profile::SECCOMP_PROFILE_ENV);
        let plan = plan_from_policy(&SandboxPolicy::default(), LandlockFsRules::deny_all())
            .expect("default policy is enforceable");
        assert_eq!(plan.cgroup.memory_max_bytes, 512 * 1024 * 1024);
        assert_eq!(plan.cgroup.cpu_quota_us, 2 * CGROUP_CPU_PERIOD_US);
        assert_eq!(plan.cgroup.cpu_period_us, CGROUP_CPU_PERIOD_US);
        assert_eq!(plan.cgroup.disk_mib, Some(5120));
        assert!(!plan.cgroup.disk_enforced);
        assert!(plan.landlock_net_deny);
        assert_eq!(plan.network, NetworkPolicy::Deny);
        assert_eq!(plan.landlock_fs.path_count(), 0);
        assert!(plan.namespaces.uts && plan.namespaces.ipc && plan.namespaces.cgroup);
        assert!(plan.namespaces.net);
        assert!(!plan.namespaces.mount && !plan.namespaces.pid && !plan.namespaces.user);
        assert!(plan.namespaces.pivot_rootfs.is_none());
        assert!(plan.namespaces.uid_maps.is_empty() && plan.namespaces.gid_maps.is_empty());
        assert_eq!(plan.seccomp.profile, SeccompProfile::BlockDangerous);
    }

    #[test]
    fn user_ns_id_map_proc_line() {
        let m = UserNsIdMap {
            inside: 0,
            outside: 501,
            count: 1,
        };
        assert_eq!(m.proc_line(), "0 501 1");
    }

    #[test]
    fn allow_network_disables_landlock_net_deny_flag() {
        let policy = SandboxPolicy {
            network: NetworkPolicy::Allow,
            ..SandboxPolicy::default()
        };
        let plan = plan_from_policy(&policy, LandlockFsRules::deny_all()).unwrap();
        assert!(!plan.landlock_net_deny);
        assert!(!plan.namespaces.net);
    }

    #[test]
    fn zero_cpu_is_bad_request() {
        let policy = SandboxPolicy {
            cpu_cores: 0,
            ..SandboxPolicy::default()
        };
        let err = plan_from_policy(&policy, LandlockFsRules::deny_all()).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn zero_memory_is_bad_request() {
        let policy = SandboxPolicy {
            memory_mib: 0,
            ..SandboxPolicy::default()
        };
        let err = plan_from_policy(&policy, LandlockFsRules::deny_all()).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn fs_rules_path_count() {
        let rules = LandlockFsRules {
            read_only: vec![PathBuf::from("/usr")],
            read_write: vec![PathBuf::from("/tmp/work")],
            execute: vec![PathBuf::from("/bin")],
        };
        assert_eq!(rules.path_count(), 3);
    }

    #[test]
    fn namespace_plan_requested_names() {
        let n = NamespacePlan::process_local(true);
        assert_eq!(n.requested_names(), vec!["uts", "ipc", "cgroup", "net"]);
    }
}

//! Linux namespace unshare / PID-ns fork / USER maps / mount pivot_root enforcement.
//!
//! wraps: nix 0.31 — `nix::sched::unshare` / `CloneFlags` + `unistd::fork`
//!   / `signal::kill` + bounded SIGKILL teardown (nix-rust/nix)
//! wraps: nix 0.31 — `mount` + `unistd::pivot_root` (via [`super::namespaces_pivot`])
//!
//! # Availability
//!
//! Live apply requires **all** of:
//! - feature `sandbox-namespaces`
//! - `target_os = "linux"`
//! - sufficient privileges / user-ns capability for the requested flags
//!
//! Otherwise [`apply`] returns
//! [`PhenoError::UnsupportedPlatform`](eidolon_core::PhenoError) with
//! [`codes::SANDBOX_NAMESPACES_UNSUPPORTED`](crate::codes::SANDBOX_NAMESPACES_UNSUPPORTED).
//!
//! # PID namespace (fork required)
//!
//! `unshare(CLONE_NEWPID)` does **not** move the calling process into the new
//! PID ns. When [`NamespacePlan::pid`](crate::enforcement::NamespacePlan::pid)
//! is set we **unshare then fork**:
//!
//! - **Child** becomes PID 1 in the new ns and returns from [`apply`] with
//!   [`NamespaceStatus::pid_ns_isolates_caller`] `true`,
//!   [`NamespaceStatus::isolated_child_host_pid`] `None`, and a
//!   [`PidNsSetup::Child`] write end. After later hooks succeed it must
//!   [`PidNsChildSetup::report_success`] then park; on failure it must
//!   [`PidNsChildSetup::report_failure_and_exit`].
//! - **Parent** returns with `pid_ns_isolates_caller = true`,
//!   `isolated_child_host_pid = Some(host_pid)`, and a [`PidNsSetup::Parent`]
//!   read end. Supervisors **must** [`PidNsParentSetup::await_ready`] before
//!   claiming start success — the byte is written only after child setup
//!   (cgroup/Landlock/seccomp) finishes. Fail-loud on failure / EOF.
//!
//! Requesting PID ns never silently skips the fork — capability denial fails
//! loud. macOS / feature-off always fail loud.
//!
//! # Mount pivot_root
//!
//! When [`NamespacePlan::pivot_rootfs`](crate::enforcement::NamespacePlan::pivot_rootfs)
//! is `Some`, after `unshare(CLONE_NEWNS)` the process that will run sandbox
//! work performs `pivot_root` into that rootfs (see [`super::namespaces_pivot`]).
//! With PID ns, **only the child** pivots (parent keeps the host root). Without
//! PID ns, the calling process pivots in-place — callers must expect `/` to
//! change. Env: `EIDOLON_SANDBOX_NS_PIVOT=1` +
//! `EIDOLON_SANDBOX_NS_PIVOT_ROOTFS=<dir>` (sets `mount` + `pivot_rootfs` in
//! [`plan_from_policy`](crate::enforcement::plan_from_policy)). Fail-loud when
//! pivot is requested but rootfs/env/mount/capability is missing.
//!
//! # USER namespace maps
//!
//! When [`NamespacePlan::user`](crate::enforcement::NamespacePlan::user) is set,
//! after `unshare(CLONE_NEWUSER)` we write `setgroups=deny` and
//! `uid_map` / `gid_map` (see [`super::namespaces_user`]). Status
//! [`NamespaceStatus::user_ns_mapped`] mirrors `disk_enforced` honesty.
//! Env: `EIDOLON_SANDBOX_NS_USER=1` (+ optional `EIDOLON_SANDBOX_NS_UID_MAP` /
//! `EIDOLON_SANDBOX_NS_GID_MAP` as `inside:outside:count` or comma-separated
//! ranges; `EIDOLON_SANDBOX_NS_USER_SUBIDS=1` reads `/etc/subuid` +
//! `/etc/subgid` for the login user). Single identity `0:<euid|egid>:1` writes
//! `/proc` directly; multi-range maps invoke `newuidmap` / `newgidmap` (path
//! override: `EIDOLON_SANDBOX_NEWUIDMAP` / `EIDOLON_SANDBOX_NEWGIDMAP`). Fail-loud
//! on map write failure. Maps are written before PID fork when both are requested
//! so the child inherits a mapped USER ns.
//!
//! # Honesty gaps
//!
//! - **EnforcingSandbox + PID ns**: guest `SandboxAutomator` lifecycle stays on
//!   the parent; enforced isolation (ns + later hooks) runs in the tracked
//!   child (documented subset).

use std::path::PathBuf;

use eidolon_core::error::PhenoError;
use eidolon_core::Result;

use crate::codes;
use crate::enforcement::plan::{EnforcementPlan, UserNsIdMap};

/// Env var: truthy enables [`crate::enforcement::NamespacePlan::pid`] in
/// [`crate::enforcement::plan_from_policy`].
pub const NS_PID_ENV: &str = "EIDOLON_SANDBOX_NS_PID";

/// Env var: truthy requests mount-ns `pivot_root` (requires
/// [`NS_PIVOT_ROOTFS_ENV`]).
pub const NS_PIVOT_ENV: &str = "EIDOLON_SANDBOX_NS_PIVOT";

/// Env var: absolute path to the rootfs directory for `pivot_root`.
pub const NS_PIVOT_ROOTFS_ENV: &str = "EIDOLON_SANDBOX_NS_PIVOT_ROOTFS";

/// Env var: truthy enables [`crate::enforcement::NamespacePlan::user`] in
/// [`crate::enforcement::plan_from_policy`].
pub const NS_USER_ENV: &str = "EIDOLON_SANDBOX_NS_USER";

/// Env var: optional uid map `inside:outside:count` or comma-separated ranges
/// (default at apply: `0:<euid>:1`).
pub const NS_UID_MAP_ENV: &str = "EIDOLON_SANDBOX_NS_UID_MAP";

/// Env var: optional gid map `inside:outside:count` or comma-separated ranges
/// (default at apply: `0:<egid>:1`).
pub const NS_GID_MAP_ENV: &str = "EIDOLON_SANDBOX_NS_GID_MAP";

/// Env var: when truthy (and uid/gid map env unset), read `/etc/subuid` and
/// `/etc/subgid` for the login user and map identity + subordinate ranges via
/// `newuidmap` / `newgidmap`.
pub const NS_USER_SUBIDS_ENV: &str = "EIDOLON_SANDBOX_NS_USER_SUBIDS";

/// Default subordinate ID file paths (Linux shadow-utils convention).
pub const SUBUID_FILE: &str = "/etc/subuid";
pub const SUBGID_FILE: &str = "/etc/subgid";

/// Result of a successful namespace apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceStatus {
    /// Human-readable list of flags that were passed to `unshare`.
    pub flags_applied: Vec<&'static str>,
    /// `true` when PID ns was requested **and** the fork completed so sandbox
    /// work can run as PID 1 in the child (disk_enforced-style honesty).
    /// Always `false` when PID was not requested. Never `true` for unshare
    /// without fork.
    pub pid_ns_isolates_caller: bool,
    /// Host PID of the isolated child when this status is from the **parent**
    /// after a PID-ns fork. `None` for the child, or when PID ns was not
    /// requested.
    pub isolated_child_host_pid: Option<u32>,
    /// `true` when mount `pivot_root` completed in **this** process (or, for
    /// [`run_in_namespaces`] parent status after a successful PID-ns child that
    /// pivoted). Always `false` when pivot was not requested. Never `true` on
    /// silent skip.
    pub pivot_root_applied: bool,
    /// `true` when USER ns maps were written in **this** process after
    /// `unshare(CLONE_NEWUSER)`. Always `false` when user ns was not
    /// requested. Never `true` on silent skip.
    pub user_ns_mapped: bool,
}

impl NamespaceStatus {
    /// Parent side of a PID-ns fork (holds the child host PID).
    pub fn is_pid_ns_parent(&self) -> bool {
        self.isolated_child_host_pid.is_some()
    }

    /// Child side of a PID-ns fork (this process is in the new PID ns).
    pub fn is_pid_ns_child(&self) -> bool {
        self.pid_ns_isolates_caller && self.isolated_child_host_pid.is_none()
    }
}

/// Result of [`apply`]: status plus optional PID-ns setup handshake end.
#[derive(Debug)]
pub struct NamespaceApply {
    pub status: NamespaceStatus,
    /// Present after a PID-ns fork. Parent reads; child writes. Absent when
    /// PID ns was not requested.
    pub setup: Option<PidNsSetup>,
}

/// One end of the post-fork setup handshake pipe (byte `0` = ok, `1` = fail).
#[derive(Debug)]
pub enum PidNsSetup {
    Parent(PidNsParentSetup),
    Child(PidNsChildSetup),
}

/// Parent read end: block until the child reports setup success/failure.
#[derive(Debug)]
pub struct PidNsParentSetup {
    read_end: std::fs::File,
    child_host_pid: u32,
}

/// Child write end: report setup outcome before parking or `_exit`.
#[derive(Debug)]
pub struct PidNsChildSetup {
    write_end: std::fs::File,
}

impl PidNsParentSetup {
    /// Wait for the child's setup byte. Returns `Ok` only on byte `0`.
    /// On failure/EOF, reaps the child (if still alive) and fails loud.
    pub fn await_ready(mut self) -> Result<()> {
        #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
        {
            namespaces_apply::await_pid_ns_setup(&mut self.read_end, self.child_host_pid)
        }
        #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
        {
            let _ = (&mut self.read_end, self.child_host_pid);
            Err(unsupported(
                "PidNsParentSetup::await_ready requires Linux + feature `sandbox-namespaces`",
            ))
        }
    }
}

impl PidNsChildSetup {
    /// Report successful remaining-hook setup to the parent, then return so
    /// the caller can [`park_until_signal`].
    pub fn report_success(mut self) -> Result<()> {
        #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
        {
            namespaces_apply::report_pid_ns_setup(&mut self.write_end, true)
        }
        #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
        {
            let _ = &mut self.write_end;
            Err(unsupported(
                "PidNsChildSetup::report_success requires Linux + feature `sandbox-namespaces`",
            ))
        }
    }

    /// Report setup failure to the parent and `_exit(1)`. Never returns.
    pub fn report_failure_and_exit(mut self) -> ! {
        #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
        {
            let _ = namespaces_apply::report_pid_ns_setup(&mut self.write_end, false);
            // Safety: child must not unwind into a duplicated Rust runtime.
            unsafe { libc::_exit(1) }
        }
        #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
        {
            let _ = &mut self.write_end;
            // Off-platform: should be unreachable after fail-loud apply.
            std::process::exit(1)
        }
    }
}

/// Host probe: namespaces appear available (Linux + feature).
pub fn namespaces_ready() -> bool {
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        std::path::Path::new("/proc/self/ns/uts").exists()
    }
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        false
    }
}

/// Apply namespace unshare described by `plan`.
///
/// When `plan.namespaces.pid` is set on Linux, **forks** after `unshare` so the
/// child is PID 1 in the new PID ns (see module docs). The returned
/// [`NamespaceApply::setup`] pipe end must be consumed: parent
/// [`PidNsParentSetup::await_ready`], child [`PidNsChildSetup::report_success`]
/// (then park) or [`PidNsChildSetup::report_failure_and_exit`]. When
/// `pivot_rootfs` is set, the process that runs sandbox work pivots after
/// mount unshare (child when PID ns). Fail-loud off-platform / feature-off /
/// capability denial — no silent skip when PID ns or pivot is requested.
pub fn apply(plan: &EnforcementPlan) -> Result<NamespaceApply> {
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        namespaces_apply::apply_plan(&plan.namespaces)
    }
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        let _ = plan;
        Err(unsupported(
            "namespace apply requires Linux + feature `sandbox-namespaces` \
             (this host/build cannot unshare isolation namespaces; PID ns \
             fork / mount pivot_root are unavailable off Linux)",
        ))
    }
}

/// Run `work` inside the namespace plan (sync helper).
///
/// When `plan.pid` is set, unshare + fork: **child** runs `work` as PID 1
/// (fails if `getpid() != 1`), reports via pipe, then `_exit`s; **parent**
/// waits and returns status with `isolated_child_host_pid` set.
/// When PID is not requested, unshare in-process and run `work` here.
/// Pivot (when planned) runs in the same process that executes `work`.
///
/// Prefer this for one-shot hermetic checks. Long-lived enforcement uses
/// [`apply`] + supervisor tracking (see [`super::EnforcingSandbox`]).
pub fn run_in_namespaces(
    plan: &crate::enforcement::NamespacePlan,
    work: impl FnOnce() -> Result<()>,
) -> Result<NamespaceStatus> {
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        namespaces_apply::run_in_plan(plan, work)
    }
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        let _ = (plan, work);
        Err(unsupported(
            "run_in_namespaces requires Linux + feature `sandbox-namespaces`",
        ))
    }
}

/// Signal a PID-ns child previously returned in
/// [`NamespaceStatus::isolated_child_host_pid`].
///
/// Sends SIGTERM, polls with `WNOHANG` for a short grace period, then escalates
/// to SIGKILL + `waitpid` so teardown cannot hang on an ignored/custom SIGTERM.
pub fn terminate_isolated_child(host_pid: u32) -> Result<()> {
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        namespaces_apply::terminate_child(host_pid)
    }
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        let _ = host_pid;
        Err(unsupported(
            "terminate_isolated_child requires Linux + feature `sandbox-namespaces`",
        ))
    }
}

/// Block the current thread until a terminating signal (PID-ns child park).
pub fn park_until_signal() -> ! {
    #[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
    {
        namespaces_apply::park_until_signal()
    }
    #[cfg(not(all(feature = "sandbox-namespaces", target_os = "linux")))]
    {
        // Off-platform: should never be reached after fail-loud apply.
        loop {
            std::thread::park();
        }
    }
}

fn unsupported(message: impl Into<String>) -> PhenoError {
    PhenoError::unsupported_platform(codes::SANDBOX_NAMESPACES_UNSUPPORTED, message)
}

#[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
mod namespaces_apply;
mod namespaces_validate;

#[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
pub use namespaces_apply::{apply_plan, run_in_plan};
pub use namespaces_validate::{
    env_pivot_rootfs_path, env_requests_pid_ns, env_requests_pivot_root, env_requests_user_ns,
    env_requests_user_subids, env_user_ns_maps, parse_id_map_spec, parse_subid_file,
    resolve_pivot_rootfs,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_pid_default_off() {
        // Do not assert global env; only that the parser accepts known truthy forms.
        let _ = env_requests_pid_ns();
        let _ = env_requests_pivot_root();
        let _ = env_requests_user_ns();
    }

    #[test]
    fn ns_env_names_stable() {
        assert_eq!(NS_PID_ENV, "EIDOLON_SANDBOX_NS_PID");
        assert_eq!(NS_PIVOT_ENV, "EIDOLON_SANDBOX_NS_PIVOT");
        assert_eq!(NS_PIVOT_ROOTFS_ENV, "EIDOLON_SANDBOX_NS_PIVOT_ROOTFS");
        assert_eq!(NS_USER_ENV, "EIDOLON_SANDBOX_NS_USER");
        assert_eq!(NS_UID_MAP_ENV, "EIDOLON_SANDBOX_NS_UID_MAP");
        assert_eq!(NS_GID_MAP_ENV, "EIDOLON_SANDBOX_NS_GID_MAP");
        assert_eq!(NS_USER_SUBIDS_ENV, "EIDOLON_SANDBOX_NS_USER_SUBIDS");
    }

    #[test]
    fn parse_id_map_spec_single_and_multi() {
        let one = parse_id_map_spec("0:1000:1").expect("one");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].outside, 1000);
        let two = parse_id_map_spec("0:1000:1,1:100000:65536").expect("two");
        assert_eq!(two.len(), 2);
        assert_eq!(two[1].count, 65_536);
    }

    #[test]
    fn parse_subid_file_finds_user_range() {
        let content = "root:0:0\nalice:100000:65536\nbob:200000:65536\n";
        let got = parse_subid_file(content, "alice")
            .expect("parse")
            .expect("range");
        assert_eq!(got, (100_000, 65_536));
        assert!(parse_subid_file(content, "missing")
            .expect("parse")
            .is_none());
    }

    #[test]
    fn parse_subid_file_ignores_comments_and_blanks() {
        let content = "# comment\n\nalice:100000:65536 # trailing\n";
        let got = parse_subid_file(content, "alice")
            .expect("parse")
            .expect("range");
        assert_eq!(got, (100_000, 65_536));
    }

    #[test]
    fn parse_id_map_spec_rejects_empty() {
        let err = parse_id_map_spec("  ,  ").unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn resolve_pivot_rootfs_off_when_not_requested() {
        assert!(resolve_pivot_rootfs(false, None).unwrap().is_none());
        assert!(resolve_pivot_rootfs(false, Some("/x")).unwrap().is_none());
    }

    #[test]
    fn resolve_pivot_rootfs_requires_path_when_requested() {
        let err = resolve_pivot_rootfs(true, None).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
        let err = resolve_pivot_rootfs(true, Some("")).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn resolve_pivot_rootfs_accepts_path() {
        let p = resolve_pivot_rootfs(true, Some("/var/lib/eidolon/rootfs")).unwrap();
        assert_eq!(p.unwrap().as_os_str(), "/var/lib/eidolon/rootfs");
    }
}

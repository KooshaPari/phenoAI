//! Composition-time wiring: apply enabled enforcement on sandbox lifecycle start.
//!
//! [`EnforcingSandbox`] is a hexagonal decorator over any [`SandboxAutomator`].
//! Default [`crate::SandboxClient`] stays fail-loud for `start`; live backends
//! (`DockerSandboxClient`, nanoVM/KVM hermetic clients) keep their own
//! isolation. Wrap explicitly when the **host process** must be constrained
//! after a successful inner `start`.
//!
//! # Feature honesty
//!
//! | Build | Behavior on `start` after inner succeeds |
//! |---|---|
//! | No enforcement features | Fail-loud [`codes::SANDBOX_ENFORCEMENT_DISABLED`](crate::codes::SANDBOX_ENFORCEMENT_DISABLED) |
//! | Any of landlock / cgroup / namespaces / seccomp | Apply each enabled hook in order (fail-loud off Linux) |
//!
//! Order: namespaces → cgroup → Landlock → seccomp.
//!
//! When PID ns is requested (`NamespacePlan.pid` / `EIDOLON_SANDBOX_NS_PID`),
//! namespaces apply **forks** with a setup pipe: the parent awaits the child's
//! post-hook ready byte before returning `Ok`; the child continues remaining
//! hooks, reports success (then parks) or failure (`_exit`). `stop` terminates
//! that child (SIGTERM → bounded SIGKILL). Guest automator lifecycle stays on
//! the parent (documented subset).
//!
//! Mount `pivot_root` (`NamespacePlan.pivot_rootfs` /
//! `EIDOLON_SANDBOX_NS_PIVOT` + `EIDOLON_SANDBOX_NS_PIVOT_ROOTFS`) runs after
//! `CLONE_NEWNS` in the process that executes sandbox work (PID-ns **child**
//! when forked). Status `pivot_root_applied` mirrors `disk_enforced` honesty.
//!
//! Docker / container engine isolation is **not** Landlock — do not wrap a
//! Docker client expecting guest FS rules; map CPU/memory via engine host
//! config (already done by `DockerSandboxClient`).

use super::{
    plan_from_policy, CgroupStatus, LandlockFsRules, LandlockStatus, NamespaceStatus, SeccompStatus,
};
#[cfg(feature = "sandbox-cgroup")]
use super::apply_cgroup;
#[cfg(feature = "sandbox-landlock")]
use super::apply_landlock;
#[cfg(feature = "sandbox-namespaces")]
use super::{
    apply_namespaces, park_until_signal, terminate_isolated_child, PidNsSetup,
};
#[cfg(feature = "sandbox-seccomp")]
use super::apply_seccomp;
use crate::codes;
use eidolon_core::security::{validate_sandbox_id, SandboxPolicy};
use eidolon_core::error::PhenoError;
use eidolon_core::traits::{ResourceUsage, SandboxAutomator, SandboxMetadata};
use eidolon_core::{AutomationEvent, Result};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Partial or full isolation result from [`apply_enabled_enforcement`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppliedEnforcement {
    pub namespaces: Option<NamespaceStatus>,
    pub cgroup: Option<CgroupStatus>,
    pub landlock: Option<LandlockStatus>,
    pub seccomp: Option<SeccompStatus>,
}

fn any_enforcement_feature() -> bool {
    cfg!(feature = "sandbox-landlock")
        || cfg!(feature = "sandbox-cgroup")
        || cfg!(feature = "sandbox-namespaces")
        || cfg!(feature = "sandbox-seccomp")
}

/// Apply whichever enforcement features are compiled in.
///
/// Pure planning always runs first. Live apply is feature-gated; with **no**
/// enforcement features this returns fail-loud
/// [`codes::SANDBOX_ENFORCEMENT_DISABLED`] — never a silent skip.
///
/// PID ns: after fork, the **parent** blocks on the setup pipe until the child
/// reports cgroup/Landlock/seccomp success (or fails loud); the **child**
/// continues those hooks, reports via pipe, then parks (never returns) or
/// `_exit`s on failure.
pub fn apply_enabled_enforcement(
    sandbox_id: &str,
    policy: &SandboxPolicy,
    fs: LandlockFsRules,
) -> Result<AppliedEnforcement> {
    validate_sandbox_id(sandbox_id)?;
    // Always validate the plan (zero ceilings → BadRequest) even when apply
    // will fail-loud for platform/feature reasons.
    let plan = plan_from_policy(policy, fs)?;

    if !any_enforcement_feature() {
        return Err(PhenoError::unsupported_platform(
            codes::SANDBOX_ENFORCEMENT_DISABLED,
            "EnforcingSandbox / apply_enabled_enforcement requires at least one of \
             `sandbox-landlock`, `sandbox-cgroup`, `sandbox-namespaces`, \
             `sandbox-seccomp` (enforcement is opt-in; default SandboxClient \
             start stays UnsupportedPlatform)",
        ));
    }

    let mut applied = AppliedEnforcement::default();

    #[cfg(feature = "sandbox-namespaces")]
    let mut pid_child_setup = None;
    #[cfg(feature = "sandbox-namespaces")]
    {
        let ns_apply = apply_namespaces(&plan)?;
        applied.namespaces = Some(ns_apply.status);
        match ns_apply.setup {
            Some(PidNsSetup::Parent(rx)) => {
                // Block until child finishes remaining hooks (or reports failure).
                rx.await_ready()?;
                return Ok(applied);
            }
            Some(PidNsSetup::Child(tx)) => {
                pid_child_setup = Some(tx);
            }
            None => {}
        }
    }

    // Remaining hooks — in the PID-ns child these run before the ready byte.
    // On failure the child must report + `_exit` (not return Err into a
    // duplicated runtime while the parent already supervises).
    let rest = (|| -> Result<()> {
        #[cfg(feature = "sandbox-cgroup")]
        {
            applied.cgroup = Some(apply_cgroup(sandbox_id, &plan)?);
        }

        #[cfg(feature = "sandbox-landlock")]
        {
            applied.landlock = Some(apply_landlock(&plan)?);
        }

        #[cfg(feature = "sandbox-seccomp")]
        {
            applied.seccomp = Some(apply_seccomp(&plan)?);
        }
        Ok(())
    })();

    #[cfg(feature = "sandbox-namespaces")]
    {
        if let Some(tx) = pid_child_setup {
            match rest {
                Ok(()) => {
                    if tx.report_success().is_err() {
                        // Parent sees EOF on the setup pipe; do not unwind a
                        // duplicated Rust runtime.
                        #[cfg(target_os = "linux")]
                        unsafe {
                            libc::_exit(1)
                        }
                        #[cfg(not(target_os = "linux"))]
                        std::process::exit(1);
                    }
                    // Stay alive so restrict_self / filters remain in force.
                    let _ = applied;
                    park_until_signal();
                }
                Err(_) => {
                    tx.report_failure_and_exit();
                }
            }
        }
    }

    rest?;
    let _ = (sandbox_id, &plan);
    Ok(applied)
}

/// Decorator that runs [`apply_enabled_enforcement`] after a successful inner
/// [`SandboxAutomator::start`].
///
/// Opt-in at composition time. Idempotent: a second `start` skips re-apply
/// (Landlock `restrict_self` / seccomp filters are irreversible; cgroup
/// membership and namespace unshare are sticky). When PID ns forked a child,
/// [`Self::stop`] terminates that child.
pub struct EnforcingSandbox<I> {
    inner: I,
    sandbox_id: String,
    policy: SandboxPolicy,
    fs: LandlockFsRules,
    applied: AtomicBool,
    /// Host PID of the PID-ns child (0 = none).
    isolated_child_host_pid: AtomicU32,
}

impl<I> std::fmt::Debug for EnforcingSandbox<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnforcingSandbox")
            .field("sandbox_id", &self.sandbox_id)
            .field("policy", &self.policy)
            .field("fs", &self.fs)
            .field("applied", &self.applied.load(Ordering::Relaxed))
            .field(
                "isolated_child_host_pid",
                &self.isolated_child_host_pid.load(Ordering::Relaxed),
            )
            .finish_non_exhaustive()
    }
}

impl<I> EnforcingSandbox<I> {
    /// Wrap `inner` so lifecycle `start` applies policy-backed enforcement.
    ///
    /// Returns [`PhenoError::BadRequest`] if `sandbox_id` fails validation.
    pub fn wrap(
        inner: I,
        sandbox_id: &str,
        policy: SandboxPolicy,
        fs: LandlockFsRules,
    ) -> Result<Self> {
        validate_sandbox_id(sandbox_id)?;
        Ok(Self {
            inner,
            sandbox_id: sandbox_id.to_string(),
            policy,
            fs,
            applied: AtomicBool::new(false),
            isolated_child_host_pid: AtomicU32::new(0),
        })
    }

    /// Convenience: wrap with [`LandlockFsRules::deny_all`].
    pub fn wrap_deny_all(inner: I, sandbox_id: &str, policy: SandboxPolicy) -> Result<Self> {
        Self::wrap(inner, sandbox_id, policy, LandlockFsRules::deny_all())
    }

    /// Declared sandbox id used for cgroup naming / validation.
    pub fn sandbox_id(&self) -> &str {
        &self.sandbox_id
    }

    /// Isolation policy applied on start.
    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    /// Filesystem rules planned for Landlock.
    pub fn fs_rules(&self) -> &LandlockFsRules {
        &self.fs
    }

    /// Whether enforcement has already been applied in this process.
    pub fn enforcement_applied(&self) -> bool {
        self.applied.load(Ordering::Acquire)
    }

    /// Host PID of the PID-ns isolated child, if any.
    pub fn isolated_child_host_pid(&self) -> Option<u32> {
        match self.isolated_child_host_pid.load(Ordering::Acquire) {
            0 => None,
            pid => Some(pid),
        }
    }

    /// Borrow the inner automator.
    pub fn inner(&self) -> &I {
        &self.inner
    }

    /// Consume the decorator and return the inner automator.
    pub fn into_inner(self) -> I {
        self.inner
    }
}

#[async_trait::async_trait]
impl<I> SandboxAutomator for EnforcingSandbox<I>
where
    I: SandboxAutomator + Send + Sync,
{
    async fn get_metadata(&self) -> Result<SandboxMetadata> {
        self.inner.get_metadata().await
    }

    async fn start(&self) -> Result<()> {
        self.inner.start().await?;
        if self.applied.load(Ordering::Acquire) {
            return Ok(());
        }
        let applied = apply_enabled_enforcement(&self.sandbox_id, &self.policy, self.fs.clone())?;
        if let Some(pid) = applied
            .namespaces
            .as_ref()
            .and_then(|n| n.isolated_child_host_pid)
        {
            self.isolated_child_host_pid.store(pid, Ordering::Release);
        }
        self.applied.store(true, Ordering::Release);
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let pid = self.isolated_child_host_pid.swap(0, Ordering::AcqRel);
        if pid != 0 {
            #[cfg(feature = "sandbox-namespaces")]
            {
                terminate_isolated_child(pid)?;
            }
            #[cfg(not(feature = "sandbox-namespaces"))]
            {
                let _ = pid;
            }
        }
        self.inner.stop().await
    }

    async fn exec(&self, cmd: &str) -> Result<String> {
        self.inner.exec(cmd).await
    }

    async fn resource_usage(&self) -> Result<ResourceUsage> {
        self.inner.resource_usage().await
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        self.inner.record_event(event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes;
    use eidolon_core::error::PhenoError;
    use eidolon_core::security::SandboxPolicy;
    use std::sync::atomic::AtomicUsize;

    struct CountingInner {
        starts: AtomicUsize,
    }

    impl CountingInner {
        fn new() -> Self {
            Self {
                starts: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl SandboxAutomator for CountingInner {
        async fn get_metadata(&self) -> Result<SandboxMetadata> {
            Ok(SandboxMetadata {
                id: "counting".into(),
                image: "test:inner".into(),
                cpu_limit: 1,
                memory_limit_mb: 128,
                disk_limit_mb: None,
            })
        }

        async fn start(&self) -> Result<()> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn stop(&self) -> Result<()> {
            Ok(())
        }

        async fn exec(&self, _cmd: &str) -> Result<String> {
            Ok(String::new())
        }

        async fn resource_usage(&self) -> Result<ResourceUsage> {
            Ok(ResourceUsage {
                cpu_percent: 0.0,
                memory_mb: 0,
                disk_mb: None,
            })
        }

        async fn record_event(&self, _event: AutomationEvent) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn wrap_rejects_empty_sandbox_id() {
        let err = EnforcingSandbox::wrap_deny_all(
            CountingInner::new(),
            "",
            SandboxPolicy::default(),
        );
        assert!(matches!(err, Err(PhenoError::BadRequest(_))));
    }

    #[test]
    fn apply_enabled_rejects_zero_cpu_before_feature_gate() {
        let policy = SandboxPolicy {
            cpu_cores: 0,
            ..SandboxPolicy::default()
        };
        let err = apply_enabled_enforcement("s1", &policy, LandlockFsRules::deny_all()).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    fn is_enforcement_fail_loud_code(code: &str) -> bool {
        matches!(
            code,
            codes::SANDBOX_ENFORCEMENT_DISABLED
                | codes::SANDBOX_LANDLOCK_UNSUPPORTED
                | codes::SANDBOX_CGROUP_UNSUPPORTED
                | codes::SANDBOX_NAMESPACES_UNSUPPORTED
                | codes::SANDBOX_SECCOMP_UNSUPPORTED
        )
    }

    #[tokio::test]
    async fn start_fail_loud_when_enforcement_unavailable() {
        let inner = CountingInner::new();
        let outer = EnforcingSandbox::wrap_deny_all(inner, "wire-1", SandboxPolicy::default())
            .expect("valid id");

        // Live Linux with enforcement features would actually restrict the
        // process — only assert fail-loud when apply cannot succeed here.
        #[cfg(not(all(
            any(
                feature = "sandbox-landlock",
                feature = "sandbox-cgroup",
                feature = "sandbox-namespaces",
                feature = "sandbox-seccomp"
            ),
            target_os = "linux"
        )))]
        {
            let err = outer.start().await.unwrap_err();
            assert_eq!(err.status_code(), 501);
            assert!(outer.inner().starts.load(Ordering::SeqCst) >= 1);
            assert!(!outer.enforcement_applied());
            let code = err.unsupported_code().expect("501 code");
            assert!(
                is_enforcement_fail_loud_code(code),
                "unexpected code {code}"
            );
        }

        #[cfg(all(
            any(
                feature = "sandbox-landlock",
                feature = "sandbox-cgroup",
                feature = "sandbox-namespaces",
                feature = "sandbox-seccomp"
            ),
            target_os = "linux"
        ))]
        {
            // Do not call start() with deny-all / net-unshare — would brick
            // the suite. Plan path is covered by plan_from_policy unit tests.
            let _ = outer;
        }
    }

    #[tokio::test]
    async fn metadata_delegates_to_inner() {
        let outer = EnforcingSandbox::wrap_deny_all(
            CountingInner::new(),
            "meta-1",
            SandboxPolicy::default(),
        )
        .unwrap();
        let meta = outer.get_metadata().await.unwrap();
        assert_eq!(meta.image, "test:inner");
    }
}

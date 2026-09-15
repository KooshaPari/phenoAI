//! Wiring tests: `EnforcingSandbox` applies enforcement on lifecycle start.
//!
//! macOS-safe: never claims Landlock/cgroup/namespace/seccomp success off
//! Linux. Live Linux apply remains in `enforcement_linux.rs` behind env gates.

use eidolon_core::error::PhenoError;
use eidolon_core::security::SandboxPolicy;
use eidolon_core::traits::{ResourceUsage, SandboxAutomator, SandboxMetadata};
use eidolon_core::{AutomationEvent, Result};
use eidolon_sandbox::codes;
use eidolon_sandbox::{
    apply_enabled_enforcement, EnforcingSandbox, LandlockFsRules, SandboxClient,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn assert_code(err: PhenoError, expected: &str) {
    assert_eq!(err.unsupported_code(), Some(expected));
    assert_eq!(err.status_code(), 501);
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

/// Inner automator that succeeds `start` (unlike default `SandboxClient`).
struct HermeticInner {
    id: String,
    started: AtomicBool,
    start_count: AtomicUsize,
}

impl HermeticInner {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            started: AtomicBool::new(false),
            start_count: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl SandboxAutomator for HermeticInner {
    async fn get_metadata(&self) -> Result<SandboxMetadata> {
        Ok(SandboxMetadata {
            id: self.id.clone(),
            image: "hermetic:test".into(),
            cpu_limit: 2,
            memory_limit_mb: 512,
            disk_limit_mb: Some(5120),
        })
    }

    async fn start(&self) -> Result<()> {
        self.start_count.fetch_add(1, Ordering::SeqCst);
        self.started.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.started.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn exec(&self, cmd: &str) -> Result<String> {
        Ok(format!("echo:{cmd}"))
    }

    async fn resource_usage(&self) -> Result<ResourceUsage> {
        Ok(ResourceUsage {
            cpu_percent: 1.0,
            memory_mb: 32,
            disk_mb: None,
        })
    }

    async fn record_event(&self, _event: AutomationEvent) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn default_sandbox_client_start_still_fail_loud() {
    let client = SandboxClient::new("stub-wire").expect("valid id");
    let err = client.start().await.unwrap_err();
    assert_code(err, codes::SANDBOX_DOCKER_STUB);
}

#[tokio::test]
async fn enforcing_wrap_fail_loud_inner_still_started() {
    let inner = HermeticInner::new("wire-ok");
    let outer = EnforcingSandbox::wrap_deny_all(inner, "wire-ok", SandboxPolicy::default())
        .expect("valid id");

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
        assert!(outer.inner().start_count.load(Ordering::SeqCst) >= 1);
        assert!(!outer.enforcement_applied());
        let code = err.unsupported_code().expect("code");
        assert!(is_enforcement_fail_loud_code(code), "got {code}");
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
        // Avoid deny-all Landlock / net-unshare on the test process.
        let _ = outer;
    }
}

#[tokio::test]
async fn enforcing_does_not_apply_when_inner_start_fails() {
    let stub = SandboxClient::new("inner-fail").expect("valid id");
    let outer = EnforcingSandbox::wrap_deny_all(stub, "inner-fail", SandboxPolicy::default())
        .expect("valid id");
    let err = outer.start().await.unwrap_err();
    // Inner fail-loud wins before enforcement apply.
    assert_code(err, codes::SANDBOX_DOCKER_STUB);
    assert!(!outer.enforcement_applied());
}

#[test]
fn apply_enabled_fail_loud_without_live_platform() {
    #[cfg(not(any(
        feature = "sandbox-landlock",
        feature = "sandbox-cgroup",
        feature = "sandbox-namespaces",
        feature = "sandbox-seccomp"
    )))]
    {
        let err = apply_enabled_enforcement(
            "no-feat",
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
        let err = apply_enabled_enforcement(
            "off-os",
            &SandboxPolicy::default(),
            LandlockFsRules::deny_all(),
        )
        .unwrap_err();
        assert_eq!(err.status_code(), 501);
        let code = err.unsupported_code().unwrap();
        assert!(is_enforcement_fail_loud_code(code), "got {code}");
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
        // Live path — do not apply deny-all / net-unshare here.
        let _ = LandlockFsRules::deny_all();
    }
}

#[test]
fn documented_enforcement_disabled_code_stable() {
    assert_eq!(
        codes::SANDBOX_ENFORCEMENT_DISABLED,
        "EIDOLON_SANDBOX_ENFORCEMENT_DISABLED"
    );
}

#[tokio::test]
async fn enforcing_delegates_metadata_and_stop() {
    let outer = EnforcingSandbox::wrap_deny_all(
        HermeticInner::new("deleg"),
        "deleg",
        SandboxPolicy::default(),
    )
    .unwrap();
    let meta = outer.get_metadata().await.unwrap();
    assert_eq!(meta.image, "hermetic:test");
    outer.stop().await.unwrap();
}

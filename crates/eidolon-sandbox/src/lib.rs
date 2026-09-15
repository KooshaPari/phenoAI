//! Eidolon Sandbox — Container and VM automation.
//!
//! # Honesty (A+ T0 / T1 / T2 slice)
//!
//! - [`SandboxClient`] — **fail-loud** for lifecycle / exec / resource probes
//!   via [`PhenoError::UnsupportedPlatform`](eidolon_core::PhenoError) with
//!   documented [`codes`]. `get_metadata` returns the *requested* policy as
//!   stub metadata only (image `"stub:latest"`); it does **not** introspect
//!   a real container.
//! - [`docker::StubDockerOrchestrator`] — fail-loud Docker trait stub
//!   (`EIDOLON_SANDBOX_DOCKER_STUB`) when feature `sandbox-docker` is off or
//!   no daemon is available.
//! - Feature `sandbox-docker`: [`docker::BollardDockerOrchestrator`] +
//!   [`docker::DockerSandboxClient`] — **live** start / stop / exec via
//!   bollard when Docker Engine answers a ping; otherwise the same fail-loud
//!   stub code (CI-friendly).
//! - Feature `sandbox-nanovm`: [`nanovm::OpsNanoVmClient`] — live when system
//!   `ops` answers `version` (hermetic start); compose
//!   [`unikernel::LaunchPlan`] via `try_from_plan`; live `ops run` when
//!   `NANOVM_INTEGRATION=1` / `UNIKERNEL_BOOT_INTEGRATION=1`. Guest `exec`
//!   uses serial/stdio ([`unikernel::exec`]) when a live child is held +
//!   `UNIKERNEL_EXEC_INTEGRATION=1`. Absent CLI → same stub.
//! - Feature `sandbox-kvm`: [`kvm::FirecrackerKvmClient`] — live when system
//!   `firecracker` answers `--version` (hermetic start); compose
//!   [`unikernel::LaunchPlan`] via `try_from_plan`; live boot when
//!   `FIRECRACKER_INTEGRATION=1` / `UNIKERNEL_BOOT_INTEGRATION=1`. Guest
//!   `exec` uses serial/stdio when a live child is held +
//!   `UNIKERNEL_EXEC_INTEGRATION=1`. Absent CLI → same.
//! - [`unikernel`] — shared rootfs / launch config + hermetic path probe
//!   (`EIDOLON_ROOTFS` / `EIDOLON_KERNEL`) + [`unikernel::boot`] plan→argv +
//!   [`unikernel::exec`] serial I/O types + [`unikernel::vsock`] NDJSON
//!   framing; missing artifacts → `EIDOLON_SANDBOX_ROOTFS_MISSING` /
//!   `_KERNEL_MISSING`. Feature `sandbox-unikernel`:
//!   [`unikernel::UnikernelGuestClient`] hermetic preflight + env-gated
//!   guest process spawn + serial/vsock exec. Feature `sandbox-vsock`:
//!   live Linux AF_VSOCK (`UNIKERNEL_VSOCK_INTEGRATION=1`). Always-on
//!   [`unikernel::pack`] hermetic packaging (manifest + stage); feature
//!   `sandbox-rootfs-pack` + `ROOTFS_PACK_INTEGRATION=1` for live
//!   `mkfs.ext4` / `virt-make-fs` / `docker export` /
//!   `DockerToExt4` compose pipelines (tar alone is Raw — Ext4 only after
//!   mkfs/virt-make-fs). Canned durable tree/disk recipe:
//!   [`unikernel::pack::build_canned_rootfs`] / `eidolon-canned-rootfs`
//!   (`docs/guides/canned-rootfs.md`; GH release Ext4 asset still optional).
//!   See `docs/reference/rootfs-pack.md`.
//! - [`nanovm::NanoVmStub`] / [`kvm::KvmStub`] / [`unikernel::UnikernelStub`] —
//!   fail-loud scaffolding when features are off or tools are missing.
//! - [`enforcement`] — **policy plan** always; live Landlock / cgroup v2 /
//!   namespace / seccomp apply behind `sandbox-landlock` / `sandbox-cgroup` /
//!   `sandbox-namespaces` / `sandbox-seccomp` on Linux; off-platform and
//!   feature-off → fail-loud `EIDOLON_SANDBOX_*_UNSUPPORTED` codes (never
//!   pretend success).
//! - [`EnforcingSandbox`] — composition decorator: after a successful inner
//!   `start`, runs [`apply_enabled_enforcement`]. Opt-in via features; none
//!   → `EIDOLON_SANDBOX_ENFORCEMENT_DISABLED`. Default [`SandboxClient`]
//!   `start` stays 501 — does **not** pretend macOS enforces isolation.
//!   Do not wrap Docker expecting guest FS rules (engine host config already
//!   maps CPU/memory).
//! - [`PlayCuaDispatcher`] — **real** dispatcher over an injected
//!   [`PlayCuaPort`] (use [`NullPlayCuaPort`] in tests; wire a real transport
//!   at composition).
//! - [`virtual_display`] — PlayCua/KDesktopVirt virtual display extract:
//!   always-on probes + hermetic Xvfb argv planning; live Linux Xvfb spawn
//!   behind `sandbox-virtual-display` + `XVFB_INTEGRATION=1`; VNC / Wayland
//!   compositor isolation fail-loud with stable codes; macOS →
//!   `EIDOLON_SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED`.
//! - Phase D session: [`session`] — memory + fail-loud unavailable; feature
//!   `sandbox-session` adds [`session::FileSessionStore`] (explicit path);
//!   feature `sandbox-session-redis` adds [`session::RedisSessionStore`]
//!   (explicit Redis URL; connect + `PING` required).
//! - Phase D audit: [`audit`] — memory + fail-loud unavailable + honesty
//!   reports + secondary query indexes (time, event type, actor, target,
//!   correlation); feature `sandbox-audit` adds [`audit::FileAuditStore`] +
//!   SHA-256 + persisted `*.jsonl.idx.json` sidecar (corrupt index →
//!   `EIDOLON_SANDBOX_AUDIT_INDEX`).
//! - [`AuditingSandbox`] / [`SandboxClient::with_audit`] — composition wire:
//!   `record_event` appends via [`AuditEngine`]. Default [`SandboxClient`]
//!   without audit stays log-only (does not claim durable audit). Unavailable
//!   store when audit is wired → fail-loud `EIDOLON_SANDBOX_AUDIT_BACKEND`.
//!   Tests: [`SandboxClient::with_memory_audit`] / [`AuditingSandbox::wrap_memory`].
//!
//! Source satellites (do not unarchive routinely):
//! [KDesktopVirt](https://github.com/KooshaPari/KDesktopVirt) (archived),
//! [PlayCua](https://github.com/KooshaPari/PlayCua) (active).
//!
//! # Input validation contract
//!
//! Every `SandboxAutomator` impl in this crate gates [`SandboxClient`]
//! through [`eidolon_core::security`] before any payload touches a
//! real backend:
//!
//! - [`SandboxClient::new`] validates the `sandbox_id` up front via
//!   [`validate_sandbox_id`](eidolon_core::security::validate_sandbox_id),
//!   so a malformed id can never reach the (eventual) container /
//!   microVM lifecycle hooks.
//! - [`SandboxClient::exec`] validates the command string via
//!   [`validate_exec_cmd`](eidolon_core::security::validate_exec_cmd),
//!   so shell metacharacter injection is rejected at the trait
//!   boundary rather than silently passed to a future `sh -c`.
//!
//! Both validators emit [`PhenoError::BadRequest`] for malformed input
//! and [`PhenoError::Forbidden`] for policy rejections — see the
//! [`eidolon_core::security`] module docs for the rule set.

use eidolon_core::error::PhenoError;
use eidolon_core::security::{validate_exec_cmd, validate_sandbox_id, SandboxPolicy};
use eidolon_core::traits::{ResourceUsage, SandboxAutomator, SandboxMetadata};
use eidolon_core::{AutomationEvent, Result};
use std::sync::Arc;

pub mod audit;
mod audit_index;
pub mod auditing;
pub mod codes;
pub mod docker;
pub mod enforcement;
pub mod kvm;
pub mod nanovm;
pub mod playcua_dispatcher;
pub mod session;
#[cfg(feature = "sandbox-session-redis")]
mod session_redis;
pub mod unikernel;
pub mod virtual_display;

pub use audit::{
    audit_entry_from_automation, AuditConfig, AuditEngine, AuditEntry, AuditEntryBuilder,
    AuditEventType, AuditQueryIndexes, AuditSeverity, AuditStore, ComplianceReport,
    ComplianceSummary, IntegrityResult, MemoryAuditStore, OutcomeResult, QueryFilter,
    RetentionPolicy, SharedAuditStore, UnavailableAuditStore, INDEX_VERSION,
};
pub use auditing::AuditingSandbox;
#[cfg(feature = "sandbox-audit")]
pub use audit::FileAuditStore;
pub use docker::{
    probe as docker_probe, ContainerConfig, DockerOrchestrator, PortMapping, ResourceSnapshot,
    StubDockerOrchestrator,
};
#[cfg(feature = "sandbox-docker")]
pub use docker::{BollardDockerOrchestrator, DockerSandboxClient};
pub use kvm::{probe as kvm_probe, KvmBackend, KvmStub};
#[cfg(feature = "sandbox-kvm")]
pub use kvm::FirecrackerKvmClient;
pub use nanovm::{probe as nanovm_probe, NanoVmBackend, NanoVmStub};
#[cfg(feature = "sandbox-nanovm")]
pub use nanovm::OpsNanoVmClient;
pub use unikernel::{
    boot as unikernel_boot, exec as unikernel_exec, pack as unikernel_pack, probe as unikernel_probe,
    vsock as unikernel_vsock, KernelConfig, LaunchPlan, RootfsConfig, RootfsFormat, UnikernelBackend,
    UnikernelLaunchConfig, UnikernelStub,
};
#[cfg(feature = "sandbox-unikernel")]
pub use unikernel::UnikernelGuestClient;
pub use playcua_dispatcher::{NullPlayCuaPort, PlayCuaConfig, PlayCuaDispatcher, PlayCuaPort};
pub use virtual_display::{
    probe as virtual_display_probe, VirtualDisplayConfig, VirtualDisplayHandle,
    VirtualDisplayKind, VirtualDisplayManager, VirtualDisplayStub, XVFB_INTEGRATION_ENV,
};
pub use session::{
    MemorySessionStore, SessionData, SessionRecord, SessionResources, SessionStorage,
    SessionStore, UnavailableSessionStore,
};
#[cfg(feature = "sandbox-session")]
pub use session::FileSessionStore;
#[cfg(feature = "sandbox-session-redis")]
pub use session::RedisSessionStore;
pub use enforcement::{
    apply_cgroup, apply_enabled_enforcement, apply_isolation, apply_landlock, apply_namespaces,
    apply_seccomp, cgroup_ready, env_requests_pid_ns, env_requests_pivot_root, env_requests_user_ns,
    env_requests_user_subids, env_user_ns_maps, landlock_ready, load_oci_allowlist_file,
    namespaces_ready, parse_id_map_spec, parse_subid_file, oci_default_allowlist, park_until_signal, parse_oci_allowlist, parse_seccomp_profile_spec,
    plan_from_policy, profile_label, resolve_pivot_rootfs, resolve_seccomp_profile,
    run_in_namespaces, seccomp_ready, terminate_isolated_child, AppliedEnforcement, CgroupPlan,
    CgroupStatus, EnforcementPlan, EnforcingSandbox, IsolationStatus, LandlockFsRules,
    LandlockStatus, NamespaceApply, NamespacePlan, NamespaceStatus, PidNsChildSetup,
    PidNsParentSetup, PidNsSetup, SeccompPlan, SeccompProfile, SeccompStatus, UserNsIdMap,
    CGROUP_CPU_PERIOD_US, NS_GID_MAP_ENV, NS_PID_ENV, NS_PIVOT_ENV, NS_PIVOT_ROOTFS_ENV,
    NS_UID_MAP_ENV, NS_USER_ENV, NS_USER_SUBIDS_ENV, SECCOMP_PROFILE_ENV, SUBGID_FILE, SUBUID_FILE,
};
#[cfg(all(feature = "sandbox-namespaces", target_os = "linux"))]
pub use enforcement::{NEWGIDMAP_ENV, NEWUIDMAP_ENV};


/// Resolve the stable [`codes`] token for a backend label.
pub fn code_for_backend(backend: &str) -> &'static str {
    match backend.to_ascii_lowercase().as_str() {
        "docker" | "container" | "bollard" => codes::SANDBOX_DOCKER_STUB,
        "nanovm" | "nanovms" | "ops" => codes::SANDBOX_NANOVM_STUB,
        "kvm" | "firecracker" | "fc" => codes::SANDBOX_KVM_STUB,
        "unikernel" | "rootfs" | "microvm" => codes::SANDBOX_UNIKERNEL_STUB,
        "rootfs-pack" | "rootfs_pack" | "pack" => codes::SANDBOX_ROOTFS_PACK_STUB,
        _ => codes::SANDBOX_OTHER_STUB,
    }
}

/// Sandbox automation implementer (**fail-loud** lifecycle stub).
///
/// For live paths: enable `sandbox-docker` / `sandbox-nanovm` / `sandbox-kvm`
/// and use [`DockerSandboxClient`] / [`OpsNanoVmClient`] /
/// [`FirecrackerKvmClient`] when host tools answer.
///
/// # Audit
///
/// Default construction has **no** audit store: `record_event` is log-only and
/// does not claim durable persistence. Wire audit explicitly via
/// [`Self::with_memory_audit`], [`Self::with_audit`], or wrap any
/// [`SandboxAutomator`] with [`AuditingSandbox`].
#[derive(Debug)]
pub struct SandboxClient {
    sandbox_id: String,
    /// Isolation policy the future Docker / nanoVMs / Firecracker
    /// backend should enforce. Carried as a value today so the public
    /// API stays stable when real isolation backends land; the stub
    /// impls read it only to populate the metadata response.
    policy: SandboxPolicy,
    /// Declared backend label (e.g. `"docker"`, `"nanovm"`, `"kvm"`).
    backend: String,
    code: &'static str,
    /// When set, `record_event` appends via [`AuditEngine`] (fail-loud if
    /// the store rejects append). `None` = log-only stub.
    audit: Option<AuditEngine<SharedAuditStore>>,
}

impl SandboxClient {
    /// Construct a sandbox client with the default isolation policy and
    /// `"docker"` backend label.
    ///
    /// Returns [`PhenoError::BadRequest`] if `sandbox_id` fails
    /// [`validate_sandbox_id`](eidolon_core::security::validate_sandbox_id)
    /// — i.e. is empty, too long, contains a forbidden byte, or starts
    /// with `-`.
    pub fn new(sandbox_id: &str) -> Result<Self> {
        Self::with_backend(sandbox_id, "docker", SandboxPolicy::default())
    }

    /// Construct a sandbox client with an explicit isolation policy
    /// (backend defaults to `"docker"`).
    ///
    /// Same id-validation contract as [`SandboxClient::new`]. Use this
    /// when the caller wants to express non-default isolation
    /// guarantees (e.g. `cpu_cores = 4`, `network = Allow` for an
    /// automation that legitimately needs egress).
    pub fn with_policy(sandbox_id: &str, policy: SandboxPolicy) -> Result<Self> {
        Self::with_backend(sandbox_id, "docker", policy)
    }

    /// Construct a sandbox client with an explicit backend label and policy.
    ///
    /// The error code is chosen from the **label**, so callers can assert
    /// `EIDOLON_SANDBOX_DOCKER_STUB` vs `EIDOLON_SANDBOX_NANOVM_STUB` /
    /// `EIDOLON_SANDBOX_KVM_STUB`.
    pub fn with_backend(sandbox_id: &str, backend: &str, policy: SandboxPolicy) -> Result<Self> {
        validate_sandbox_id(sandbox_id)?;
        Ok(Self {
            sandbox_id: sandbox_id.to_string(),
            policy,
            backend: backend.to_string(),
            code: code_for_backend(backend),
            audit: None,
        })
    }

    /// Wire an injected [`AuditStore`] so `record_event` appends via
    /// [`AuditEngine`]. Unavailable / rejecting stores fail loud — events
    /// are never silently dropped when audit is wired.
    pub fn with_audit(mut self, store: Arc<dyn AuditStore>, config: AuditConfig) -> Self {
        self.audit = Some(AuditEngine::from_arc(store, config));
        self
    }

    /// Wire [`MemoryAuditStore`] (default for tests / in-process sinks).
    pub fn with_memory_audit(self) -> Self {
        self.with_audit(Arc::new(MemoryAuditStore::new()), AuditConfig::default())
    }

    /// Borrow the wired audit engine, if any.
    pub fn audit_engine(&self) -> Option<&AuditEngine<SharedAuditStore>> {
        self.audit.as_ref()
    }

    /// Borrow the configured isolation policy (for inspection / logging
    /// by callers that don't yet need to dispatch a real backend).
    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    /// Declared backend label (e.g. `"docker"`, `"nanovm"`). Informative only.
    pub fn backend(&self) -> &str {
        &self.backend
    }

    /// Stable [`codes`] token this stub emits on lifecycle failures.
    pub fn unsupported_code(&self) -> &'static str {
        self.code
    }

    fn unsupported(&self, method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            self.code,
            format!(
                "eidolon-sandbox::SandboxClient::{method} is not implemented \
                 (backend={:?}; for live Docker enable feature `sandbox-docker` \
                 and use DockerSandboxClient; for nanoVMs enable \
                 `sandbox-nanovm` + OpsNanoVmClient; for KVM enable \
                 `sandbox-kvm` + FirecrackerKvmClient — see \
                 docs/EXTRACTION_PLAN.md and \
                 docs/consolidation/KDesktopVirt-to-Eidolon.md; do not \
                 unarchive KDesktopVirt routinely; use PlayCuaDispatcher \
                 with an injected PlayCuaPort for real dispatch)",
                self.backend
            ),
        )
    }
}

#[async_trait::async_trait]
impl SandboxAutomator for SandboxClient {
    async fn get_metadata(&self) -> Result<SandboxMetadata> {
        // Honest stub: surfaces *requested* policy only. Does not probe
        // a live container/VM. Image label is explicitly `stub:latest`.
        Ok(SandboxMetadata {
            id: self.sandbox_id.clone(),
            image: "stub:latest".to_string(),
            cpu_limit: self.policy.cpu_cores,
            memory_limit_mb: self.policy.memory_mib,
            disk_limit_mb: self.policy.disk_mib,
        })
    }

    async fn start(&self) -> Result<()> {
        Err(self.unsupported("start"))
    }

    async fn stop(&self) -> Result<()> {
        Err(self.unsupported("stop"))
    }

    async fn exec(&self, cmd: &str) -> Result<String> {
        validate_exec_cmd(cmd)?;
        Err(self.unsupported("exec"))
    }

    async fn resource_usage(&self) -> Result<ResourceUsage> {
        Err(self.unsupported("resource_usage"))
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        if let Some(engine) = &self.audit {
            engine
                .record_automation_event(&self.sandbox_id, &event)
                .await?;
            return Ok(());
        }
        // Honest stub: no durable audit unless wired via with_audit /
        // AuditingSandbox. Do not pretend events were persisted.
        log::debug!("Recorded sandbox event (no audit store wired): {:?}", event);
        Ok(())
    }
}

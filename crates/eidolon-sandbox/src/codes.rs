//! Documented machine-readable error codes for `eidolon-sandbox`.
//!
//! These codes appear in [`PhenoError::UnsupportedPlatform`](eidolon_core::PhenoError)
//! and are stable for callers / agents to match on. Prefer matching on the code
//! string rather than parsing the human message.
//!
//! Do **not** unarchive KDesktopVirt for routine work — copy patterns only in
//! scoped extraction PRs (see `docs/EXTRACTION_PLAN.md` and
//! `docs/consolidation/KDesktopVirt-to-Eidolon.md`). PlayCua stays active.

/// Docker orchestration / `SandboxClient` lifecycle path is a fail-loud stub
/// when feature `sandbox-docker` is off or the Docker Engine is unreachable.
///
/// With `sandbox-docker` enabled and a reachable daemon, prefer
/// [`crate::docker::BollardDockerOrchestrator`] /
/// [`crate::docker::DockerSandboxClient`] for live start/stop/exec.
///
/// Extraction targets (archived KDesktopVirt — copy patterns only):
/// `src/containerization.rs`, `src/virtualization.rs` → `docker/` behind
/// `sandbox-docker` (bollard).
pub const SANDBOX_DOCKER_STUB: &str = "EIDOLON_SANDBOX_DOCKER_STUB";

/// nanoVMs backend path is a fail-loud stub when feature `sandbox-nanovm` is
/// off or the NanoVMs `ops` CLI is unreachable.
///
/// With `sandbox-nanovm` enabled and a reachable `ops`, prefer
/// [`crate::nanovm::OpsNanoVmClient`] for hermetic version/start; guest
/// serial exec when a live child is held + `UNIKERNEL_EXEC_INTEGRATION=1`.
///
/// Extraction targets: KVirtualStage / KDesktopVirt nanoVM patterns →
/// `nanovm` module (Phase 3). Do not unarchive KDesktopVirt routinely.
pub const SANDBOX_NANOVM_STUB: &str = "EIDOLON_SANDBOX_NANOVM_STUB";

/// KVM / Firecracker backend path is a fail-loud stub when feature
/// `sandbox-kvm` is off or the Firecracker CLI is unreachable.
///
/// With `sandbox-kvm` enabled and a reachable `firecracker`, prefer
/// [`crate::kvm::FirecrackerKvmClient`] for hermetic version/start; guest
/// serial exec when a live child is held + `UNIKERNEL_EXEC_INTEGRATION=1`.
///
/// Extraction targets: KDesktopVirt virtualization + PlayCua VM session
/// patterns → `kvm` module. Do not unarchive KDesktopVirt routinely.
pub const SANDBOX_KVM_STUB: &str = "EIDOLON_SANDBOX_KVM_STUB";

/// Unknown / other sandbox backend label (not docker, nanovm, or kvm).
pub const SANDBOX_OTHER_STUB: &str = "EIDOLON_SANDBOX_OTHER_STUB";

/// No session persistence backend configured ([`crate::session::UnavailableSessionStore`]).
///
/// Prefer [`crate::session::MemorySessionStore`], feature `sandbox-session` +
/// [`crate::session::FileSessionStore`] (explicit path), or feature
/// `sandbox-session-redis` + [`crate::session::RedisSessionStore`] (explicit URL).
pub const SANDBOX_SESSION_BACKEND: &str = "EIDOLON_SANDBOX_SESSION_BACKEND";

/// Session storage I/O / parse failure (surfaced as [`PhenoError::Internal`](eidolon_core::PhenoError)
/// with this token embedded in the message).
///
/// Also used for Redis command / serialize failures after a successful connect
/// ([`crate::session::RedisSessionStore`] behind `sandbox-session-redis`).
pub const SANDBOX_SESSION_IO: &str = "EIDOLON_SANDBOX_SESSION_IO";

/// Redis session store URL / connection failure
/// ([`crate::session::RedisSessionStore`] behind `sandbox-session-redis`).
///
/// Emitted when the Redis URL is missing/empty, the URL cannot be parsed, the
/// TCP connect fails, or `PING` does not succeed. Fail-loud — never pretend an
/// empty in-memory success. Prefer an explicit URL via
/// [`crate::session::RedisSessionStore::connect`] or `EIDOLON_REDIS_URL` /
/// `REDIS_URL` via [`crate::session::RedisSessionStore::connect_from_env`].
pub const SANDBOX_SESSION_REDIS: &str = "EIDOLON_SANDBOX_SESSION_REDIS";

/// No audit backend configured ([`crate::audit::UnavailableAuditStore`]).
///
/// Prefer [`crate::audit::MemoryAuditStore`] or feature `sandbox-audit` +
/// [`crate::audit::FileAuditStore`] with an explicit path.
pub const SANDBOX_AUDIT_BACKEND: &str = "EIDOLON_SANDBOX_AUDIT_BACKEND";

/// Audit storage I/O / parse failure (surfaced as [`PhenoError::Internal`](eidolon_core::PhenoError)
/// with this token embedded in the message).
pub const SANDBOX_AUDIT_IO: &str = "EIDOLON_SANDBOX_AUDIT_IO";

/// Audit query index sidecar is corrupt or disagrees with the entry log
/// ([`crate::audit_index::AuditQueryIndexes`]).
///
/// Emitted when a persisted `*.jsonl.idx.json` fails validation against the
/// JSONL log. Fail-loud — never silently ignore a mismatched index. When the
/// sidecar is absent, indexes are rebuilt from entries (same as session
/// `session_index` recovery).
pub const SANDBOX_AUDIT_INDEX: &str = "EIDOLON_SANDBOX_AUDIT_INDEX";

/// Process-local Landlock/cgroup/namespace/seccomp enforcement was requested
/// but **no** enforcement feature is compiled in.
///
/// Emitted by [`crate::enforcement::apply_enabled_enforcement`] /
/// [`crate::EnforcingSandbox`] when all of `sandbox-landlock`,
/// `sandbox-cgroup`, `sandbox-namespaces`, and `sandbox-seccomp` are off.
/// Enforcement is **opt-in** via those features; default
/// [`crate::SandboxClient::start`] remains [`SANDBOX_DOCKER_STUB`] (or the
/// backend stub code) — it does not silently skip isolation.
pub const SANDBOX_ENFORCEMENT_DISABLED: &str = "EIDOLON_SANDBOX_ENFORCEMENT_DISABLED";

/// Linux Landlock filesystem enforcement is unavailable.
///
/// Emitted when feature `sandbox-landlock` is off, the host is not Linux,
/// the running kernel lacks Landlock ABI support, or the ruleset cannot be
/// created. Callers must treat this as fail-loud — never pretend isolation.
///
/// With `sandbox-landlock` on Linux + a Landlock-capable kernel, prefer
/// [`crate::enforcement::apply_landlock`] (or wrap with
/// [`crate::EnforcingSandbox`]).
pub const SANDBOX_LANDLOCK_UNSUPPORTED: &str = "EIDOLON_SANDBOX_LANDLOCK_UNSUPPORTED";

/// Linux cgroup v2 resource-limit enforcement is unavailable.
///
/// Emitted when feature `sandbox-cgroup` is off, the host is not Linux,
/// cgroup v2 is not mounted / writable, or the process cannot join a
/// child cgroup. Fail-loud — do not claim CPU/memory caps are enforced.
///
/// With `sandbox-cgroup` on Linux + writable cgroup v2, prefer
/// [`crate::enforcement::apply_cgroup`].
pub const SANDBOX_CGROUP_UNSUPPORTED: &str = "EIDOLON_SANDBOX_CGROUP_UNSUPPORTED";

/// Linux cgroup v2 disk / I/O limit cannot be applied.
///
/// Emitted when [`SandboxPolicy::disk_mib`](eidolon_core::security::SandboxPolicy)
/// is `Some` but no block device is resolved (`EIDOLON_CGROUP_DISK_DEV` /
/// `EIDOLON_CGROUP_DISK_PATH` / mountinfo detect) or `io.max` cannot be
/// written. cgroup v2 has no portable capacity controller — Eidolon maps
/// `disk_mib` → `io.max` rbps/wbps. Set `disk_mib: None` to skip I/O
/// throttling. Fail-loud — never silently ignore a requested disk ceiling.
pub const SANDBOX_CGROUP_DISK_UNAVAILABLE: &str = "EIDOLON_SANDBOX_CGROUP_DISK_UNAVAILABLE";

/// Linux namespace unshare / PID-ns fork / mount pivot_root enforcement is
/// unavailable.
///
/// Emitted when feature `sandbox-namespaces` is off, the host is not Linux,
/// `unshare` / fork is denied (capabilities / user-ns), PID ns was requested
/// but fork could not complete, or pivot_root was requested but failed
/// (missing rootfs / mount ns / capability). Fail-loud — do not claim
/// UTS/IPC/net/PID/pivot isolation. Successful PID isolation sets
/// [`crate::enforcement::NamespaceStatus::pid_ns_isolates_caller`]; successful
/// pivot sets [`crate::enforcement::NamespaceStatus::pivot_root_applied`].
///
/// With `sandbox-namespaces` on Linux, prefer
/// [`crate::enforcement::apply_namespaces`].
pub const SANDBOX_NAMESPACES_UNSUPPORTED: &str = "EIDOLON_SANDBOX_NAMESPACES_UNSUPPORTED";

/// Linux seccomp-bpf enforcement is unavailable.
///
/// Emitted when feature `sandbox-seccomp` is off, the host is not Linux
/// (little-endian), the arch is unsupported by seccompiler, or filter
/// install fails. Fail-loud — do not claim syscall filtering.
///
/// Profile selection: `EIDOLON_SECCOMP_PROFILE=block-dangerous` (default) \|
/// `oci-default` \| path to OCI/Docker seccomp JSON. Invalid profiles fail at
/// plan time with [`PhenoError::BadRequest`](eidolon_core::PhenoError), not
/// this code.
///
/// With `sandbox-seccomp` on Linux, prefer
/// [`crate::enforcement::apply_seccomp`].
pub const SANDBOX_SECCOMP_UNSUPPORTED: &str = "EIDOLON_SANDBOX_SECCOMP_UNSUPPORTED";

/// Guest rootfs / disk image path is missing or not a regular file.
///
/// Emitted by [`crate::unikernel::RootfsConfig::require_present`] /
/// [`crate::unikernel::LaunchPlan`] when neither an explicit path nor
/// `EIDOLON_ROOTFS` resolves to an existing file. Fail-loud — never invent
/// a default rootfs.
pub const SANDBOX_ROOTFS_MISSING: &str = "EIDOLON_SANDBOX_ROOTFS_MISSING";

/// Guest kernel image path is missing or not a regular file.
///
/// Emitted when a [`crate::unikernel::KernelConfig`] is required (or
/// `EIDOLON_KERNEL` is set) but the path does not exist. Fail-loud.
pub const SANDBOX_KERNEL_MISSING: &str = "EIDOLON_SANDBOX_KERNEL_MISSING";

/// Unikernel / microVM guest launch path is a fail-loud stub.
///
/// Emitted when feature `sandbox-unikernel` is off, the host CLI for the
/// chosen backend (`ops` / `firecracker`) is unreachable, guest spawn fails,
/// or guest resource probes are not yet wired. Rootfs validation,
/// plan→argv builders ([`crate::unikernel::boot`]), hermetic preflight, and
/// serial exec types ([`crate::unikernel::exec`]) may still succeed via
/// [`crate::unikernel::LaunchPlan`].
///
/// With `sandbox-unikernel` + present rootfs + reachable CLI, prefer
/// [`crate::unikernel::UnikernelGuestClient`]. Live process spawn requires
/// `UNIKERNEL_BOOT_INTEGRATION=1` (or `FIRECRACKER_INTEGRATION=1` /
/// `NANOVM_INTEGRATION=1`). Guest serial exec requires a live child +
/// `UNIKERNEL_EXEC_INTEGRATION=1` (see [`SANDBOX_GUEST_NOT_RUNNING`] /
/// [`SANDBOX_GUEST_IO_UNAVAILABLE`]).
pub const SANDBOX_UNIKERNEL_STUB: &str = "EIDOLON_SANDBOX_UNIKERNEL_STUB";

/// Guest exec requested but no live guest child is held.
///
/// Hermetic `start` (boot env unset) validates artifacts/CLI only — it does
/// **not** spawn a process. Callers must enable boot integration, `start()`,
/// and hold a child before [`SandboxAutomator::exec`](eidolon_core::traits::SandboxAutomator::exec).
pub const SANDBOX_GUEST_NOT_RUNNING: &str = "EIDOLON_SANDBOX_GUEST_NOT_RUNNING";

/// Guest exec I/O transport unavailable or env-gated off.
///
/// Emitted when serial/stdio pipes are missing, `UNIKERNEL_EXEC_INTEGRATION`
/// is unset, vsock is requested without `sandbox-vsock` + Linux +
/// `UNIKERNEL_VSOCK_INTEGRATION=1`, AF_VSOCK connect/read fails, or a serial
/// read times out / fails. Prefer serial unless `EIDOLON_VSOCK_CID` +
/// `EIDOLON_VSOCK_PORT` select [`crate::unikernel::exec::GuestIoTransport::Vsock`].
/// See `docs/reference/unikernel-vsock-protocol.md`.
pub const SANDBOX_GUEST_IO_UNAVAILABLE: &str = "EIDOLON_SANDBOX_GUEST_IO_UNAVAILABLE";

/// Rootfs / guest image packaging live path is gated (feature / env off).
///
/// Emitted when feature `sandbox-rootfs-pack` is off or
/// `ROOTFS_PACK_INTEGRATION` is unset. Hermetic
/// [`crate::unikernel::pack::HermeticPackBuilder`] (validate + stage +
/// manifest JSON) still works without the feature. When the feature + env
/// are on, missing tools use [`SANDBOX_ROOTFS_PACK_TOOL_MISSING`] and
/// command failures use [`SANDBOX_ROOTFS_PACK_IO`] — not this stub.
///
/// Prefer [`crate::unikernel::pack::live_pack`] only with
/// `sandbox-rootfs-pack` + `ROOTFS_PACK_INTEGRATION=1` + present tools.
pub const SANDBOX_ROOTFS_PACK_STUB: &str = "EIDOLON_SANDBOX_ROOTFS_PACK_STUB";

/// A required rootfs packaging host tool is missing (`mkfs.ext4`,
/// `virt-make-fs`, `docker`, …).
///
/// Emitted by [`crate::unikernel::pack::live_pack`] / [`crate::unikernel::pack::tools`]
/// when the tool for the chosen [`crate::unikernel::pack::PackMethod`] is not
/// on `PATH` (or the matching `EIDOLON_MKFS` / `EIDOLON_VIRT_MAKE_FS` /
/// `EIDOLON_DOCKER` override). Fail-loud — never invent a silent fallback.
pub const SANDBOX_ROOTFS_PACK_TOOL_MISSING: &str = "EIDOLON_SANDBOX_ROOTFS_PACK_TOOL_MISSING";

/// Rootfs package manifest / staging I/O failure (surfaced as
/// [`PhenoError::Internal`](eidolon_core::PhenoError) with this token embedded
/// in the message).
pub const SANDBOX_ROOTFS_PACK_IO: &str = "EIDOLON_SANDBOX_ROOTFS_PACK_IO";

/// `eidolon-vsock-agent` binary required for rootfs bake is missing.
///
/// Emitted by [`crate::unikernel::pack::bake_agent`] /
/// [`crate::unikernel::pack::canned`] when bake / canned build is requested
/// and neither an explicit path, `EIDOLON_VSOCK_AGENT`, cargo discovery, nor
/// (optional) `EIDOLON_VSOCK_AGENT_URL`+`SHA256` pin fetch yields a binary.
/// Fail-loud — never skip bake silently.
pub const SANDBOX_VSOCK_AGENT_MISSING: &str = "EIDOLON_SANDBOX_VSOCK_AGENT_MISSING";

/// Published Ext4 rootfs release asset cannot be resolved or verified.
///
/// Emitted by [`crate::unikernel::pack::release_asset`] when the pin is
/// unpublished, env/cache miss, SHA-256 mismatch, or fetch fails. Fail-loud —
/// never invent a silent empty disk. See `docs/guides/gh-ext4-release.md`.
pub const SANDBOX_ROOTFS_RELEASE_UNAVAILABLE: &str = "EIDOLON_SANDBOX_ROOTFS_RELEASE_UNAVAILABLE";

/// Virtual display manager is unsupported on this host OS (macOS / Windows).
///
/// Emitted by [`crate::virtual_display::VirtualDisplayManager`] /
/// [`crate::virtual_display::VirtualDisplayStub`] on non-Linux platforms.
/// Hermetic macOS CI must match this code — never pretend an Xvfb session exists.
pub const SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED: &str =
    "EIDOLON_SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED";

/// Live virtual display spawn is gated off (feature / integration env).
///
/// Emitted when feature `sandbox-virtual-display` is off or
/// `XVFB_INTEGRATION` is unset on Linux. Hermetic probe + `plan_xvfb` still
/// work; `start` does not spawn without the gate.
pub const SANDBOX_VIRTUAL_DISPLAY_STUB: &str = "EIDOLON_SANDBOX_VIRTUAL_DISPLAY_STUB";

/// System `Xvfb` binary not found on `PATH`.
///
/// Emitted by [`crate::virtual_display::xvfb::plan_xvfb`] /
/// [`crate::virtual_display::VirtualDisplayManager::start`] when Linux lacks
/// `Xvfb` (override via `EIDOLON_XVFB`). Fail-loud — never invent `:99`.
pub const SANDBOX_XVFB_MISSING: &str = "EIDOLON_SANDBOX_XVFB_MISSING";

/// Xvfb child process spawn or teardown I/O failure (surfaced as
/// [`PhenoError::Internal`](eidolon_core::PhenoError) with this token embedded
/// in the message).
pub const SANDBOX_XVFB_SPAWN_IO: &str = "EIDOLON_SANDBOX_XVFB_SPAWN_IO";

/// VNC isolation (x11vnc / TigerVNC attach) is not wired.
///
/// Emitted when callers request `VirtualDisplayConfig.vnc_port` or
/// [`crate::virtual_display::vnc::start_vnc`]. Probes may succeed; live RFB
/// attach is not claimed.
pub const SANDBOX_VNC_UNSUPPORTED: &str = "EIDOLON_SANDBOX_VNC_UNSUPPORTED";

/// Isolated Wayland compositor sessions (Weston / headless) are not wired.
///
/// Emitted by [`crate::virtual_display::wayland::start_wayland_compositor`] or
/// when `VirtualDisplayConfig.wayland_isolation` is true. Host Wayland portal
/// desktop automation lives in `eidolon-desktop`; this code is sandbox isolation.
pub const SANDBOX_WAYLAND_COMPOSITOR_UNSUPPORTED: &str =
    "EIDOLON_SANDBOX_WAYLAND_COMPOSITOR_UNSUPPORTED";


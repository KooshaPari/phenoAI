//! Desktop security capability hooks (A+ Phase F).
//!
//! # Status
//!
//! Phase F resolves overlap with [`eidolon_core::security`] **without**
//! unarchiving KDesktopVirt. Sandbox input validation and resource policy
//! live in core; this module owns desktop capability allow-lists and a
//! lightweight rate-limit gate. Optional vault/OAuth live behind
//! `desktop-security-vault` / `desktop-security-oauth` (see
//! [`crate::security_vault`] / [`crate::security_oauth`]).
//!
//! # Overlap audit (archived `src/security_framework.rs`)
//!
//! | KDesktopVirt type / concern | Eidolon home | Phase F action |
//! |---|---|---|
//! | `ResourceLimits` (cpu/mem/disk/net) | [`SandboxPolicy`](eidolon_core::security::SandboxPolicy) + [`NetworkPolicy`](eidolon_core::security::NetworkPolicy) | **Dedupe** — do not re-define |
//! | Sandbox id / exec string hygiene | [`validate_sandbox_id`](eidolon_core::security::validate_sandbox_id) / [`validate_exec_cmd`](eidolon_core::security::validate_exec_cmd) | **Wire** via [`DesktopSecurityGate`] defaults |
//! | `AccessController` / `allowed_operations` | [`PolicySecurityGate`] | **Port** allow-list + deny semantics |
//! | Failed-attempt lockout | [`PolicySecurityGate`] rate window | **Port** lightweight |
//! | `EncryptedVault` / AES-GCM / Argon2 | [`crate::security_vault`] (`desktop-security-vault`) | **Port** optional feature |
//! | `OAuthManager` / PKCE | [`crate::security_oauth`] (`desktop-security-oauth`) | **Port** PKCE helper (no embedded browser) |
//! | `AuditLogger` / compliance export | Phase D → `eidolon-sandbox::audit` | **Complete** |
//! | `SecurityEngine` monolith | — | **Do not port** — compose traits instead |
//!
//! See `docs/EXTRACTION_PLAN.md` Phase F and
//! `docs/consolidation/KDesktopVirt-to-Eidolon.md`.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::security::{validate_exec_cmd, validate_sandbox_id};
use eidolon_core::Result;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Well-known desktop capability tokens (dotted `domain.action`).
///
/// Prefer these constants over free-form strings so allow-lists stay auditable.
pub mod caps {
    /// Inject pointer / mouse events.
    pub const POINTER_INJECT: &str = "pointer.inject";
    /// Inject keyboard / text events.
    pub const TEXT_INJECT: &str = "text.inject";
    /// Capture a screenshot.
    pub const SCREENSHOT: &str = "screenshot.capture";
    /// Start / stop screen recording.
    pub const RECORDING: &str = "recording.control";
    /// Read viewport / display metadata.
    pub const VIEWPORT_READ: &str = "viewport.read";
    /// Validate a sandbox id via core (desktop→sandbox bridge).
    pub const SANDBOX_ID: &str = "sandbox.id";
    /// Validate an exec command via core (desktop→sandbox bridge).
    pub const SANDBOX_EXEC: &str = "sandbox.exec";

    /// Capabilities that belong in `eidolon-core::security` / sandbox policy —
    /// never reimplemented here.
    pub const CORE_OWNED: &[&str] = &[SANDBOX_ID, SANDBOX_EXEC];

    /// Default desktop allow-list for a permissive local automation gate.
    pub const DEFAULT_DESKTOP: &[&str] = &[
        POINTER_INJECT,
        TEXT_INJECT,
        SCREENSHOT,
        RECORDING,
        VIEWPORT_READ,
        SANDBOX_ID,
        SANDBOX_EXEC,
    ];

    /// Archived KDesktopVirt surfaces we intentionally refuse to host here.
    pub const NOT_PORTED: &[&str] = &[
        "vault.encrypt",
        "vault.decrypt",
        "oauth.register",
        "oauth.flow",
        "password.hash",
        "password.verify",
        "audit.export",
        "security.engine",
    ];
}

/// Maximum length of a capability token, in bytes.
pub const CAPABILITY_MAX_LEN: usize = 64;

/// Validate a capability token shape (independent of allow-list policy).
///
/// Rules:
/// - Non-empty, ≤ [`CAPABILITY_MAX_LEN`].
/// - ASCII lowercase alphanumeric segments separated by `.` (at least one `.`).
/// - No leading/trailing `.`, no consecutive `..`.
///
/// Returns [`PhenoError::BadRequest`] on malformed input.
pub fn validate_capability_name(capability: &str) -> Result<()> {
    if capability.is_empty() {
        return Err(PhenoError::BadRequest(
            "capability must not be empty".into(),
        ));
    }
    if capability.len() > CAPABILITY_MAX_LEN {
        return Err(PhenoError::BadRequest(format!(
            "capability length {} exceeds maximum {CAPABILITY_MAX_LEN}",
            capability.len()
        )));
    }
    if !capability.contains('.')
        || capability.starts_with('.')
        || capability.ends_with('.')
        || capability.contains("..")
    {
        return Err(PhenoError::BadRequest(format!(
            "capability {capability:?} must be dotted domain.action \
             (lowercase alphanumeric segments)"
        )));
    }
    for byte in capability.bytes() {
        let allowed = byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || byte == b'.'
            || byte == b'_'
            || byte == b'-';
        if !allowed {
            return Err(PhenoError::BadRequest(format!(
                "capability contains forbidden byte 0x{byte:02x}"
            )));
        }
    }
    Ok(())
}

/// Capability gate for desktop-side privilege / rate-limit checks.
///
/// Sandbox string hygiene is **not** reimplemented here — default methods
/// delegate to [`eidolon_core::security`].
pub trait DesktopSecurityGate: Send + Sync {
    /// Returns Ok when the operation is allowed; otherwise fail-loud.
    fn check_capability(&self, capability: &str) -> Result<()>;

    /// Validate a sandbox id via core (no local duplicate rules).
    fn check_sandbox_id(&self, id: &str) -> Result<()> {
        validate_sandbox_id(id)
    }

    /// Validate an exec command via core (no local duplicate rules).
    fn check_exec_cmd(&self, cmd: &str) -> Result<()> {
        validate_exec_cmd(cmd)
    }
}

/// Fail-loud gate — always [`codes::DESKTOP_SECURITY_UNAVAILABLE`].
///
/// Use when no policy has been configured (CI default / composition until a
/// [`PolicySecurityGate`] is injected).
#[derive(Debug, Default, Clone)]
pub struct SecurityHooksStub;

impl SecurityHooksStub {
    pub fn new() -> Self {
        Self
    }
}

impl DesktopSecurityGate for SecurityHooksStub {
    fn check_capability(&self, capability: &str) -> Result<()> {
        // Still validate shape so callers get 400 for garbage, 501 for stub.
        validate_capability_name(capability)?;
        if caps::NOT_PORTED.contains(&capability) {
            return Err(PhenoError::unsupported_platform(
                codes::DESKTOP_SECURITY_UNAVAILABLE,
                format!(
                    "DesktopSecurityGate::check_capability({capability:?}) \
                     refused — KDesktopVirt vault/OAuth not ported \
                     (Phase F: use eidolon-core::security + PolicySecurityGate; \
                     Phase D audit: eidolon-sandbox::audit)"
                ),
            ));
        }
        Err(PhenoError::unsupported_platform(
            codes::DESKTOP_SECURITY_UNAVAILABLE,
            format!(
                "DesktopSecurityGate::check_capability({capability:?}) unavailable — \
                 inject PolicySecurityGate for allow-list enforcement \
                 (Phase F; core validators via check_sandbox_id / check_exec_cmd)"
            ),
        ))
    }
}

/// Allow-list + sliding-window rate limit gate (Phase F port of AccessController
/// semantics without vault/OAuth coupling).
#[derive(Debug)]
pub struct PolicySecurityGate {
    allowed: Vec<String>,
    max_per_window: u32,
    window: Duration,
    hits: Mutex<HashMap<String, Vec<Instant>>>,
}

impl PolicySecurityGate {
    /// Build a gate with an explicit allow-list and rate-limit window.
    ///
    /// `max_per_window` of `0` disables rate limiting.
    pub fn new(
        allowed: impl IntoIterator<Item = impl Into<String>>,
        max_per_window: u32,
        window: Duration,
    ) -> Self {
        Self {
            allowed: allowed.into_iter().map(Into::into).collect(),
            max_per_window,
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// Permissive local-automation defaults ([`caps::DEFAULT_DESKTOP`]),
    /// 60 checks / 60s per capability.
    pub fn permissive_desktop() -> Self {
        Self::new(caps::DEFAULT_DESKTOP.iter().copied(), 60, Duration::from_secs(60))
    }

    /// Empty allow-list — every well-formed capability is [`PhenoError::Forbidden`].
    pub fn deny_all() -> Self {
        Self::new(std::iter::empty::<String>(), 0, Duration::from_secs(60))
    }

    fn record_and_check_rate(&self, capability: &str) -> Result<()> {
        if self.max_per_window == 0 {
            return Ok(());
        }
        let mut guard = self
            .hits
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        let entries = guard.entry(capability.to_string()).or_default();
        entries.retain(|t| now.duration_since(*t) < self.window);
        if entries.len() as u32 >= self.max_per_window {
            return Err(PhenoError::Forbidden(format!(
                "capability {capability:?} rate-limited \
                 (max {} per {:?})",
                self.max_per_window, self.window
            )));
        }
        entries.push(now);
        Ok(())
    }
}

impl DesktopSecurityGate for PolicySecurityGate {
    fn check_capability(&self, capability: &str) -> Result<()> {
        validate_capability_name(capability)?;

        if caps::NOT_PORTED.contains(&capability) {
            return Err(PhenoError::unsupported_platform(
                codes::DESKTOP_SECURITY_UNAVAILABLE,
                format!(
                    "capability {capability:?} is archived KDesktopVirt surface \
                     (vault/OAuth/engine) — not ported into eidolon-desktop; \
                     prefer eidolon-core::security; audit.export → eidolon-sandbox::audit"
                ),
            ));
        }

        if !self.allowed.iter().any(|a| a == capability) {
            return Err(PhenoError::Forbidden(format!(
                "capability {capability:?} not in allow-list"
            )));
        }

        self.record_and_check_rate(capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eidolon_core::security::{validate_exec_cmd, validate_sandbox_id};

    #[test]
    fn capability_accepts_dotted_tokens() {
        assert!(validate_capability_name(caps::POINTER_INJECT).is_ok());
        assert!(validate_capability_name("sandbox.exec").is_ok());
    }

    #[test]
    fn capability_rejects_empty_and_malformed() {
        assert!(matches!(
            validate_capability_name("").unwrap_err(),
            PhenoError::BadRequest(_)
        ));
        assert!(matches!(
            validate_capability_name("nodot").unwrap_err(),
            PhenoError::BadRequest(_)
        ));
        assert!(matches!(
            validate_capability_name(".leading").unwrap_err(),
            PhenoError::BadRequest(_)
        ));
        assert!(matches!(
            validate_capability_name("Pointer.Inject").unwrap_err(),
            PhenoError::BadRequest(_)
        ));
    }

    #[test]
    fn policy_allows_default_desktop_caps() {
        let gate = PolicySecurityGate::permissive_desktop();
        for cap in caps::DEFAULT_DESKTOP {
            gate.check_capability(cap).unwrap_or_else(|e| {
                panic!("expected allow for {cap:?}, got {e:?}")
            });
        }
    }

    #[test]
    fn policy_forbids_unknown_capability() {
        let gate = PolicySecurityGate::permissive_desktop();
        let err = gate.check_capability("admin.root").unwrap_err();
        assert!(matches!(err, PhenoError::Forbidden(_)));
    }

    #[test]
    fn policy_fail_loud_on_not_ported_vault() {
        let gate = PolicySecurityGate::permissive_desktop();
        let err = gate.check_capability("vault.encrypt").unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_SECURITY_UNAVAILABLE)
        );
        assert_eq!(err.status_code(), 501);
    }

    #[test]
    fn policy_rate_limits() {
        let gate = PolicySecurityGate::new(
            [caps::POINTER_INJECT],
            2,
            Duration::from_secs(60),
        );
        gate.check_capability(caps::POINTER_INJECT).unwrap();
        gate.check_capability(caps::POINTER_INJECT).unwrap();
        let err = gate.check_capability(caps::POINTER_INJECT).unwrap_err();
        assert!(matches!(err, PhenoError::Forbidden(_)));
    }

    #[test]
    fn gate_delegates_sandbox_validators_to_core() {
        let gate = PolicySecurityGate::deny_all();
        assert!(gate.check_sandbox_id("docker-abc").is_ok());
        assert!(matches!(
            gate.check_sandbox_id("").unwrap_err(),
            PhenoError::BadRequest(_)
        ));
        assert!(gate.check_exec_cmd("ls -la /tmp").is_ok());
        assert!(matches!(
            gate.check_exec_cmd("echo hi; rm -rf /").unwrap_err(),
            PhenoError::Forbidden(_)
        ));
        // Sanity: same rules as calling core directly.
        assert_eq!(
            gate.check_sandbox_id("--flag").unwrap_err().to_string(),
            validate_sandbox_id("--flag").unwrap_err().to_string()
        );
        assert_eq!(
            gate.check_exec_cmd("a&&b").unwrap_err().to_string(),
            validate_exec_cmd("a&&b").unwrap_err().to_string()
        );
    }

    #[test]
    fn stub_still_fail_loud_after_shape_ok() {
        let stub = SecurityHooksStub::new();
        let err = stub.check_capability(caps::POINTER_INJECT).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_SECURITY_UNAVAILABLE)
        );
    }
}

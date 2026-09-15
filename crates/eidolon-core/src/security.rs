//! Eidolon security primitives — input validation for the sandbox surface.
//!
//! # Why this module exists
//!
//! The [`SandboxAutomator`](crate::SandboxAutomator) trait exposes
//! `exec(cmd: &str)` as a raw passthrough. Today every concrete impl is
//! a stub, so unsafe input is silently swallowed — but the trait
//! surface is the eventual binding point for real Docker / nanoVMs /
//! Firecracker execution, where unvalidated command strings would be a
//! tenant-isolation incident waiting to happen (shell metacharacter
//! injection, argument confusion, resource-exhaustion payloads).
//!
//! This module defines the validation contract every
//! `SandboxAutomator::exec` impl should apply before any payload
//! touches a real backend. It is intentionally narrow — a
//! well-defined list of rules with stable error semantics — so it can
//! be reused unchanged by the desktop / mobile stages when those start
//! dispatching shell-shaped inputs.
//!
//! # Rules
//!
//! ## `sandbox_id` validation ([`validate_sandbox_id`])
//!
//! - Non-empty.
//! - At most [`SANDBOX_ID_MAX_LEN`] (64) bytes.
//! - Charset: ASCII alphanumeric, `-`, `_`, `.`.
//!   Matches the Docker container id 64-hex short form, the
//!   `name-prefix-XXXXXXXX` UUID-with-dashes convention used by
//!   nanoVMs, and the bare alphanumeric variant used by KVM domain
//!   names.
//! - Reject leading `-` to avoid the classic argv-confusion case
//!   where an id starting with `--` would be reinterpreted by a CLI
//!   wrapper as a flag.
//!
//! ## `exec_cmd` validation ([`validate_exec_cmd`])
//!
//! - Non-empty.
//! - At most [`EXEC_CMD_MAX_LEN`] (4096) bytes — bounds resource use
//!   without rejecting any realistic automation command (matches the
//!   conventional POSIX `ARG_MAX` soft-limit / Linux `MAX_CANON`
//!   boundary).
//! - Reject NUL bytes (`\0`): C-string truncation hazard.
//! - Reject newlines (`\n`, bare `\r`): would split the command into
//!   multiple shell lines if a caller accidentally routes through
//!   `sh -c`.
//! - Reject a small, well-defined set of shell metacharacter patterns
//!   that almost always indicate injection attempts on a stringly-typed
//!   API (`&&`, `||`, `|`, `>`, `<`, `$(`, backtick, `;`). Operators
//!   that need richer shell semantics should pre-`shlex` and pass argv
//!   as a structured payload, not rely on the `cmd: &str` signature.
//!
//! # Error semantics
//!
//! All failures return [`PhenoError::BadRequest`](crate::PhenoError::BadRequest)
//! (the input was malformed) or
//! [`PhenoError::Forbidden`](crate::PhenoError::Forbidden) (the input
//! was well-formed but rejected by policy). The two are separated so
//! callers can map them to 400 vs 403 at the eventual transport
//! boundary — and so policy rejections are distinguishable from
//! operator typos in audit logs.
//!
//! # Future work
//!
//! [`SandboxPolicy`] remains the typed isolation surface. Live Linux apply
//! lives in `eidolon-sandbox::enforcement` (`sandbox-landlock` /
//! `sandbox-cgroup`). Wire at composition with `EnforcingSandbox` (applies on
//! successful `start`); stub `SandboxClient::start` stays fail-loud and does
//! not pretend enforcement.

use crate::error::PhenoError;

/// Maximum length of a sandbox identifier, in bytes.
pub const SANDBOX_ID_MAX_LEN: usize = 64;

/// Maximum length of an `exec` command string, in bytes.
pub const EXEC_CMD_MAX_LEN: usize = 4096;

/// Shell metacharacter patterns that [`validate_exec_cmd`] rejects.
///
/// Kept as a module-level constant so the rule set is auditable in one
/// place and tests can iterate over it to assert no allowed command is
/// accidentally over-restricted.
pub const FORBIDDEN_EXEC_PATTERNS: &[&str] = &[
    "&&", // command chaining (left-to-right)
    "||", // command chaining (left-to-right)
    "|",  // pipe
    ">",  // output redirection
    "<",  // input redirection
    "$(", // command substitution (modern)
    "`",  // command substitution (legacy)
    ";",  // command separator
];

/// Validate a sandbox identifier.
///
/// See the [module-level docs](self) for the rule set. Returns
/// [`PhenoError::BadRequest`] on any failure.
pub fn validate_sandbox_id(id: &str) -> Result<(), PhenoError> {
    if id.is_empty() {
        return Err(PhenoError::BadRequest(
            "sandbox id must not be empty".into(),
        ));
    }
    if id.len() > SANDBOX_ID_MAX_LEN {
        return Err(PhenoError::BadRequest(format!(
            "sandbox id length {} exceeds maximum {}",
            id.len(),
            SANDBOX_ID_MAX_LEN
        )));
    }
    if id.starts_with('-') {
        return Err(PhenoError::BadRequest(
            "sandbox id must not start with '-' (argv confusion guard)".into(),
        ));
    }
    for byte in id.bytes() {
        let allowed = byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.';
        if !allowed {
            return Err(PhenoError::BadRequest(format!(
                "sandbox id contains forbidden byte 0x{byte:02x}"
            )));
        }
    }
    Ok(())
}

/// Validate an `exec(cmd)` command string.
///
/// See the [module-level docs](self) for the rule set. Returns
/// [`PhenoError::BadRequest`] for malformed input and
/// [`PhenoError::Forbidden`] for policy rejections.
pub fn validate_exec_cmd(cmd: &str) -> Result<(), PhenoError> {
    if cmd.is_empty() {
        return Err(PhenoError::BadRequest("exec cmd must not be empty".into()));
    }
    if cmd.len() > EXEC_CMD_MAX_LEN {
        return Err(PhenoError::BadRequest(format!(
            "exec cmd length {} exceeds maximum {}",
            cmd.len(),
            EXEC_CMD_MAX_LEN
        )));
    }
    for byte in cmd.bytes() {
        match byte {
            0 => return Err(PhenoError::BadRequest("exec cmd contains NUL byte".into())),
            b'\n' => {
                return Err(PhenoError::BadRequest(
                    "exec cmd must not contain newline".into(),
                ));
            }
            b'\r' if !cmd.contains('\n') => {
                // Reject bare CR (CRLF is already rejected via the LF arm).
                return Err(PhenoError::BadRequest(
                    "exec cmd must not contain bare carriage return".into(),
                ));
            }
            _ => {}
        }
    }
    for needle in FORBIDDEN_EXEC_PATTERNS {
        if cmd.contains(needle) {
            return Err(PhenoError::Forbidden(format!(
                "exec cmd contains forbidden pattern {needle:?} \
                 (use structured-argv API instead)"
            )));
        }
    }
    Ok(())
}

/// Network access policy for an isolated sandbox.
///
/// `Deny` is the safe default. Future backends may narrow egress via a
/// syscall filter (seccomp), an iptables / nftables egress proxy, or a
/// usermode networking layer (gVisor netstack).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NetworkPolicy {
    /// All egress allowed.
    Allow,
    /// All egress denied (default for untrusted automation).
    Deny,
    /// Egress denied except for an explicit allow-list (future use;
    /// the allow-list itself is a separate field on the
    /// backend-specific config, not on this enum).
    EgressAllowList,
}

/// Typed surface for sandbox isolation guarantees.
///
/// A value type only — no behavior, just shape. The stub `SandboxClient`
/// impls carry an instance so the trait surface is stable when real
/// isolation backends land and start populating these fields with
/// observed values (CPU shares from `docker stats`, memory from
/// `cgroup memory.peak`, network from `iptables -L OUTPUT`, etc.).
///
/// `Default::default()` is the safe baseline: 2 vCPU, 512 MiB RAM, 5
/// GiB disk, no network.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SandboxPolicy {
    /// CPU ceiling in cores (1 == 1 vCPU).
    pub cpu_cores: u32,
    /// Memory ceiling in MiB.
    pub memory_mib: u32,
    /// Disk ceiling in MiB, when the backend supports a writable
    /// filesystem layer.
    pub disk_mib: Option<u32>,
    /// Network access policy.
    pub network: NetworkPolicy,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            cpu_cores: 2,
            memory_mib: 512,
            disk_mib: Some(5120),
            network: NetworkPolicy::Deny,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- validate_sandbox_id ----------------------------------------

    #[test]
    fn sandbox_id_accepts_alphanumeric() {
        assert!(validate_sandbox_id("abc123").is_ok());
    }

    #[test]
    fn sandbox_id_accepts_dash_underscore_dot() {
        assert!(validate_sandbox_id("sandbox-1").is_ok());
        assert!(validate_sandbox_id("a_b").is_ok());
        assert!(validate_sandbox_id("a.b").is_ok());
        assert!(validate_sandbox_id("docker-xyz-7c9f").is_ok());
    }

    #[test]
    fn sandbox_id_accepts_max_length() {
        let id = "a".repeat(SANDBOX_ID_MAX_LEN);
        assert!(validate_sandbox_id(&id).is_ok());
    }

    #[test]
    fn sandbox_id_rejects_empty() {
        let err = validate_sandbox_id("").unwrap_err();
        assert!(
            matches!(err, PhenoError::BadRequest(_)),
            "expected BadRequest, got {err:?}"
        );
    }

    #[test]
    fn sandbox_id_rejects_too_long() {
        let id = "a".repeat(SANDBOX_ID_MAX_LEN + 1);
        let err = validate_sandbox_id(&id).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn sandbox_id_rejects_leading_dash() {
        let err = validate_sandbox_id("--flag").unwrap_err();
        assert!(
            matches!(err, PhenoError::BadRequest(_)),
            "expected BadRequest, got {err:?}"
        );
        let err = validate_sandbox_id("-abc").unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn sandbox_id_rejects_whitespace() {
        let err = validate_sandbox_id("hello world").unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn sandbox_id_rejects_shell_metacharacters() {
        for bad in [
            "a;b", "a&b", "a|b", "a$b", "a`b", "a*b", "a/b", "a\\b", "a:b",
        ] {
            let err = validate_sandbox_id(bad)
                .unwrap_err_or_else(|| panic!("expected rejection of {bad:?}"));
            assert!(
                matches!(err, PhenoError::BadRequest(_)),
                "expected BadRequest for {bad:?}, got {err:?}"
            );
        }
    }

    #[test]
    fn sandbox_id_rejects_non_ascii() {
        let err = validate_sandbox_id("café").unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    // ---- validate_exec_cmd ------------------------------------------

    #[test]
    fn exec_cmd_accepts_safe_single_token() {
        assert!(validate_exec_cmd("ls").is_ok());
        assert!(validate_exec_cmd("echo").is_ok());
        assert!(validate_exec_cmd("pwd").is_ok());
    }

    #[test]
    fn exec_cmd_accepts_safe_argv() {
        assert!(validate_exec_cmd("ls -la /tmp").is_ok());
        assert!(validate_exec_cmd("cat /etc/hostname").is_ok());
        assert!(validate_exec_cmd("echo hello world").is_ok());
    }

    #[test]
    fn exec_cmd_accepts_path_with_dash() {
        // Paths are fine; only leading `-` on the *first* token is the
        // argv-confusion guard. (Currently the validator is uniform,
        // but a future tightening could split on whitespace and only
        // gate the first token — see test below.)
        assert!(validate_exec_cmd("/usr/bin/echo hi").is_ok());
    }

    #[test]
    fn exec_cmd_accepts_max_length() {
        let cmd = "a".repeat(EXEC_CMD_MAX_LEN);
        assert!(validate_exec_cmd(&cmd).is_ok());
    }

    #[test]
    fn exec_cmd_rejects_empty() {
        let err = validate_exec_cmd("").unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn exec_cmd_rejects_too_long() {
        let cmd = "a".repeat(EXEC_CMD_MAX_LEN + 1);
        let err = validate_exec_cmd(&cmd).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn exec_cmd_rejects_nul_byte() {
        let err = validate_exec_cmd("echo\0hello").unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn exec_cmd_rejects_newline() {
        let err = validate_exec_cmd("echo hi\necho bye").unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn exec_cmd_rejects_bare_cr() {
        let err = validate_exec_cmd("echo hi\r").unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn exec_cmd_rejects_every_forbidden_pattern_as_forbidden() {
        for needle in FORBIDDEN_EXEC_PATTERNS {
            // Construct a minimal cmd that contains `needle` and is
            // otherwise well-formed so the rejection must come from
            // the pattern loop, not from a prior rule.
            let cmd = format!("echo x{needle}y");
            let err = validate_exec_cmd(&cmd).unwrap_err_or_else(|| {
                panic!("expected {needle:?} to be rejected (cmd = {cmd:?})")
            });
            assert!(
                matches!(err, PhenoError::Forbidden(_)),
                "expected Forbidden for {needle:?}, got {err:?}"
            );
        }
    }

    // ---- SandboxPolicy / NetworkPolicy ------------------------------

    #[test]
    fn sandbox_policy_default_is_safe_baseline() {
        let p = SandboxPolicy::default();
        assert_eq!(p.cpu_cores, 2);
        assert_eq!(p.memory_mib, 512);
        assert_eq!(p.disk_mib, Some(5120));
        assert_eq!(p.network, NetworkPolicy::Deny);
    }

    #[test]
    fn network_policy_distinguishes_deny_from_allow() {
        assert_ne!(NetworkPolicy::Deny, NetworkPolicy::Allow);
        assert_ne!(NetworkPolicy::Deny, NetworkPolicy::EgressAllowList);
        assert_ne!(NetworkPolicy::Allow, NetworkPolicy::EgressAllowList);
    }

    #[test]
    fn network_policy_egress_allow_list_clone_and_debug() {
        // Exercise the EgressAllowList variant's Clone, Copy, and Debug derives.
        let p = NetworkPolicy::EgressAllowList;
        let q = p; // Copy
        assert_eq!(p, q);
        let s = format!("{p:?}");
        assert!(s.contains("EgressAllowList"), "Debug output: {s}");
    }

    #[test]
    fn sandbox_policy_clone_round_trip() {
        let policy = SandboxPolicy {
            cpu_cores: 4,
            memory_mib: 1024,
            disk_mib: None,
            network: NetworkPolicy::Allow,
        };
        let cloned = policy.clone();
        assert_eq!(cloned.cpu_cores, 4);
        assert_eq!(cloned.memory_mib, 1024);
        assert_eq!(cloned.disk_mib, None);
        assert_eq!(cloned.network, NetworkPolicy::Allow);
        assert_eq!(cloned, policy);
    }

    #[test]
    fn sandbox_policy_serde_round_trip() {
        let policy = SandboxPolicy {
            cpu_cores: 1,
            memory_mib: 256,
            disk_mib: Some(2048),
            network: NetworkPolicy::EgressAllowList,
        };
        let json = serde_json::to_string(&policy).expect("serialize should succeed");
        let decoded: SandboxPolicy =
            serde_json::from_str(&json).expect("deserialize should succeed");
        assert_eq!(decoded, policy);
    }

    #[test]
    fn network_policy_serde_round_trip() {
        for variant in [
            NetworkPolicy::Allow,
            NetworkPolicy::Deny,
            NetworkPolicy::EgressAllowList,
        ] {
            let json = serde_json::to_string(&variant).expect("serialize");
            let decoded: NetworkPolicy = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(decoded, variant, "round-trip failed for {variant:?}");
        }
    }

    #[test]
    fn exec_cmd_rejects_bare_cr_inline() {
        // Bare \r without \n must be rejected by the BadRequest path.
        let err = validate_exec_cmd("hello\rworld").unwrap_err();
        assert!(
            matches!(err, PhenoError::BadRequest(_)),
            "expected BadRequest, got {err:?}"
        );
    }

    // ---- tiny extension trait so `Result::unwrap_err_or_else` reads
    // nicely in the loops above. `Result::unwrap_err` is stable since
    // 1.79 but we keep the helper readable for older readers.
    trait UnwrapErrOrElse<T> {
        fn unwrap_err_or_else<F: FnOnce()>(self, f: F) -> PhenoError;
    }
    impl<T> UnwrapErrOrElse<T> for Result<T, PhenoError> {
        fn unwrap_err_or_else<F: FnOnce()>(self, f: F) -> PhenoError {
            match self {
                Ok(_) => {
                    f();
                    unreachable!("test expected Err, got Ok")
                }
                Err(e) => e,
            }
        }
    }
}

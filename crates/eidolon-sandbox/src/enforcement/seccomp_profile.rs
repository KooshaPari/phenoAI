//! Seccomp profile selection (`EIDOLON_SECCOMP_PROFILE`).
//!
//! Values:
//! - `block-dangerous` — allow unmatched; `EPERM` a fixed high-risk set (default)
//! - `oci-default` — embedded Docker/OCI-style allowlist
//! - path to a `.json` OCI/Docker seccomp profile
//!
//! Invalid / unreadable / empty profiles fail loud with [`PhenoError::BadRequest`].

use super::seccomp_oci::{load_oci_allowlist_file, oci_default_allowlist};
use crate::enforcement::plan::SeccompProfile;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::path::{Path, PathBuf};

/// Env var selecting the seccomp profile for [`plan_from_policy`](crate::enforcement::plan_from_policy).
pub const SECCOMP_PROFILE_ENV: &str = "EIDOLON_SECCOMP_PROFILE";

/// Resolve the active profile: env override, else [`SeccompProfile::BlockDangerous`].
pub fn resolve_seccomp_profile() -> Result<SeccompProfile> {
    match std::env::var(SECCOMP_PROFILE_ENV) {
        Err(std::env::VarError::NotPresent) => Ok(SeccompProfile::BlockDangerous),
        Err(std::env::VarError::NotUnicode(_)) => Err(PhenoError::BadRequest(format!(
            "{SECCOMP_PROFILE_ENV} is not valid UTF-8"
        ))),
        Ok(raw) => parse_seccomp_profile_spec(&raw),
    }
}

/// Parse a profile spec string (`block-dangerous` | `oci-default` | path).
///
/// Validates embedded/custom allowlists at parse time so bad profiles fail
/// before apply.
pub fn parse_seccomp_profile_spec(raw: &str) -> Result<SeccompProfile> {
    let spec = raw.trim();
    if spec.is_empty() {
        return Err(PhenoError::BadRequest(format!(
            "{SECCOMP_PROFILE_ENV} must be `block-dangerous`, `oci-default`, \
             or a path to an OCI/Docker seccomp JSON file (got empty)"
        )));
    }

    match spec {
        "block-dangerous" => Ok(SeccompProfile::BlockDangerous),
        "oci-default" => {
            // Touch-parse embedded profile so corrupt embeds fail loud.
            let _ = oci_default_allowlist()?;
            Ok(SeccompProfile::OciDefault)
        }
        path => {
            let p = PathBuf::from(path);
            validate_custom_profile(&p)?;
            Ok(SeccompProfile::Custom(p))
        }
    }
}

fn validate_custom_profile(path: &Path) -> Result<()> {
    if !path.is_file() {
        return Err(PhenoError::BadRequest(format!(
            "seccomp profile path {} is not a readable file \
             ({SECCOMP_PROFILE_ENV}=block-dangerous|oci-default|/path/to.json)",
            path.display()
        )));
    }
    let _ = load_oci_allowlist_file(path)?;
    Ok(())
}

/// Human-readable profile label for status / logs.
pub fn profile_label(profile: &SeccompProfile) -> String {
    match profile {
        SeccompProfile::BlockDangerous => "block-dangerous".into(),
        SeccompProfile::OciDefault => "oci-default".into(),
        SeccompProfile::Custom(p) => p.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize env mutations across tests in this module.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn parse_builtins() {
        assert_eq!(
            parse_seccomp_profile_spec("block-dangerous").unwrap(),
            SeccompProfile::BlockDangerous
        );
        assert_eq!(
            parse_seccomp_profile_spec("oci-default").unwrap(),
            SeccompProfile::OciDefault
        );
    }

    #[test]
    fn parse_rejects_unknown_token_that_is_not_a_file() {
        let err = parse_seccomp_profile_spec("not-a-real-profile").unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn parse_rejects_empty() {
        let err = parse_seccomp_profile_spec("  ").unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn resolve_defaults_when_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: test-only; serialized by ENV_LOCK.
        std::env::remove_var(SECCOMP_PROFILE_ENV);
        assert_eq!(
            resolve_seccomp_profile().unwrap(),
            SeccompProfile::BlockDangerous
        );
    }

    #[test]
    fn resolve_reads_env() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var(SECCOMP_PROFILE_ENV, "oci-default");
        let got = resolve_seccomp_profile().unwrap();
        std::env::remove_var(SECCOMP_PROFILE_ENV);
        assert_eq!(got, SeccompProfile::OciDefault);
    }

    #[test]
    fn resolve_rejects_invalid_env() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var(SECCOMP_PROFILE_ENV, "bogus-profile-xyz");
        let err = resolve_seccomp_profile().unwrap_err();
        std::env::remove_var(SECCOMP_PROFILE_ENV);
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn profile_label_custom_uses_path() {
        let p = PathBuf::from("/tmp/seccomp.json");
        assert_eq!(
            profile_label(&SeccompProfile::Custom(p)),
            "/tmp/seccomp.json"
        );
    }
}

//! Destructive desktop action gate (mobile-style env allow-list).
//!
//! Pointer / text / live screenshot writes require
//! [`ACTIONS_ALLOW_ENV`]`=1`. Viewport reads stay ungated.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;

/// Env gate for destructive desktop actions (`EIDOLON_DESKTOP_ALLOW_ACTIONS=1`).
pub const ACTIONS_ALLOW_ENV: &str = "EIDOLON_DESKTOP_ALLOW_ACTIONS";

/// `true` when destructive desktop actions are explicitly allowed.
pub fn actions_allowed() -> bool {
    std::env::var(ACTIONS_ALLOW_ENV).ok().as_deref() == Some("1")
}

/// Fail-loud unless [`actions_allowed`].
pub fn require_actions_allowed(method: &str) -> Result<()> {
    if actions_allowed() {
        return Ok(());
    }
    Err(PhenoError::unsupported_platform(
        codes::DESKTOP_ACTIONS_GATED,
        format!(
            "desktop::{method} is gated — set {ACTIONS_ALLOW_ENV}=1 to allow \
             destructive desktop input / capture (Win32 SendInput/DXGI/GDI or \
             Linux X11 XTEST/GetImage; viewport reads stay ungated; see \
             docs/EXTRACTION_PLAN.md)"
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gated_error_uses_stable_code() {
        // When gate is off, require_* returns ACTIONS_GATED. Parallel suites
        // may set the env; only assert when currently disallowed.
        if actions_allowed() {
            return;
        }
        let err = require_actions_allowed("pointer").unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_ACTIONS_GATED)
        );
        assert_eq!(err.status_code(), 501);
    }
}

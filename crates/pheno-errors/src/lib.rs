//! Canonical error type for PlayCua (and other pheno-* binaries).
//!
//! Every binary in the fleet returns `Result<T, AppError>` from `main` so the
//! process exit code is uniform and machine-readable. The mapping is:
//!
//! | Variant                         | Exit code | BSD sysexits.h |
//! |---------------------------------|-----------|----------------|
//! | `AppError::Other`               | 1         | EX_USAGE-ish   |
//! | `AppError::Flag`                | 78        | EX_CONFIG      |
//! | `AppError::Validation`          | 65        | EX_DATAERR     |
//! | `AppError::Domain`              | 2         | (custom)       |
//! | `AppError::Storage`             | 74        | EX_IOERR       |
//!
//! The `Other` arm is the catch-all that `main` uses to coerce any
//! non-`AppError` failure (e.g. an `anyhow::Error` from a port adapter)
//! into a uniform exit code without losing the formatted chain — the
//! original message is preserved verbatim.

use thiserror::Error;

/// The canonical fleet error type. Designed to be the `Err` arm of every
/// `Result<T, E>` that bubbles all the way up to `main`.
#[derive(Debug, Error)]
pub enum AppError {
    /// Catch-all for errors that don't fit a structured variant. The
    /// wrapped `String` is the formatted error chain (typically produced
    /// by `anyhow`'s `{:#}` formatter).
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),

    /// A `pheno-flags` flag value was unparseable, or a required flag was
    /// missing. Maps to `EX_CONFIG` (78).
    #[error("invalid feature flag: {0}")]
    Flag(String),

    /// Caller-supplied input failed schema/semantic validation.
    /// Maps to `EX_DATAERR` (65).
    #[error("validation error: {0}")]
    Validation(String),

    /// Domain-layer rejection (rule violation, business-logic failure).
    /// Maps to exit code 2.
    #[error("domain error: {0}")]
    Domain(String),

    /// I/O or persistence failure. Maps to `EX_IOERR` (74).
    #[error("storage error: {0}")]
    Storage(String),
}

impl AppError {
    /// Build an `AppError::Domain` from any stringly-typed cause.
    /// Convenient for adapters that already have a `String` in hand
    /// and don't want to construct the variant via the public field.
    pub fn domain(msg: impl Into<String>) -> Self {
        AppError::Domain(msg.into())
    }

    /// Build an `AppError::Storage` from any stringly-typed cause.
    pub fn storage(msg: impl Into<String>) -> Self {
        AppError::Storage(msg.into())
    }

    /// Build an `AppError::Validation` from any stringly-typed cause.
    pub fn validation(msg: impl Into<String>) -> Self {
        AppError::Validation(msg.into())
    }

    /// Map this error to a process exit code. The mapping is stable
    /// across the fleet so orchestrators can branch on it.
    pub fn exit_code(&self) -> i32 {
        match self {
            AppError::Other(_) => 1,
            AppError::Flag(_) => 78,       // EX_CONFIG
            AppError::Validation(_) => 65, // EX_DATAERR
            AppError::Domain(_) => 2,
            AppError::Storage(_) => 74, // EX_IOERR
        }
    }
}

/// Convenience: convert an `anyhow::Error` into `AppError::Other`,
/// preserving the formatted chain (`{:#}` style — includes causes).
///
/// This is the bridge the L5 #81 main loop uses when an `anyhow::Result`
/// needs to be returned from `async fn main() -> Result<(), AppError>`.
///
/// Note: this impl is conditional on the `anyhow` feature. When the
/// feature is off (the default for non-flags consumers), callers must
/// convert with `.map_err(|e| AppError::Other(e.to_string().into()))`
/// themselves — see `into_app` in `playcua-native`'s `main.rs`.
#[cfg(feature = "anyhow")]
impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        // Keep the full chain so log lines / error responses stay useful.
        let msg = format!("{e:#}");
        AppError::Other(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_match_sysexits() {
        assert_eq!(AppError::Flag("x".into()).exit_code(), 78);
        assert_eq!(AppError::Validation("x".into()).exit_code(), 65);
        assert_eq!(AppError::Storage("x".into()).exit_code(), 74);
        assert_eq!(AppError::Domain("x".into()).exit_code(), 2);
        // String -> Box<dyn Error + Send + Sync> via std's blanket impl.
        let boxed: Box<dyn std::error::Error + Send + Sync> = "x".to_string().into();
        assert_eq!(AppError::Other(boxed).exit_code(), 1);
    }

    #[test]
    fn anyhow_round_trips_through_other() {
        #[cfg(feature = "anyhow")]
        {
            let e: anyhow::Error = anyhow::anyhow!("boom: {}", 42);
            let app: AppError = e.into();
            assert!(matches!(app, AppError::Other(_)));
            assert_eq!(app.exit_code(), 1);
        }
    }

    #[test]
    fn domain_constructor_wraps_string() {
        let e = AppError::domain("rule violated");
        assert!(matches!(e, AppError::Domain(ref s) if s == "rule violated"));
    }
}

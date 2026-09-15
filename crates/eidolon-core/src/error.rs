//! Re-export canonical error types from phenotype-error-core.
//! Provides type alias for automation-specific operations.
//!
//! `phenotype-error-core` exposes the layer-specific error enums directly
//! (e.g. `ApiError`, `DomainError`). We define the legacy umbrella alias
//! `PhenoError` and the convenience `Result` type locally so the rest of
//! the Eidolon API surface (and downstream crates like `eidolon-desktop`,
//! `eidolon-mobile`, `eidolon-sandbox`) keep their public types stable.

use phenotype_error_core::ApiError;

/// Phenotype umbrella error alias for cross-crate consumers (Sidekick, Eidolon, etc.).
///
/// Equivalent to [`ApiError`]; provided so downstream crates can `use
/// eidolon_core::PhenoError` (or `eidolon_core::error::PhenoError`)
/// without coupling to the underlying alias name.
pub type PhenoError = ApiError;

/// Convenience result type.
pub type Result<T> = std::result::Result<T, ApiError>;

/// Automation-specific result type (alias for convenience).
pub type AutomationResult<T> = Result<T>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pheno_error_is_api_error_alias() {
        let e: PhenoError = PhenoError::BadRequest("test".into());
        let as_api: ApiError = e;
        assert!(matches!(as_api, ApiError::BadRequest(_)));
    }

    #[test]
    fn result_ok_type_inference() {
        let r: Result<i32> = Ok(42);
        assert_eq!(r.unwrap(), 42);
    }

    #[test]
    fn result_err_type_inference() {
        let r: Result<()> = Err(PhenoError::Timeout);
        assert!(r.is_err());
    }

    #[test]
    fn automation_result_is_result_alias() {
        let a: AutomationResult<bool> = Ok(true);
        let r: Result<bool> = a;
        assert!(r.unwrap());
    }

    #[test]
    fn display_variants_via_alias() {
        let cases: Vec<(PhenoError, &str)> = vec![
            (PhenoError::BadRequest("x".into()), "bad request"),
            (PhenoError::Forbidden("x".into()), "forbidden"),
            (PhenoError::NotFound("x".into()), "not found"),
            (PhenoError::Internal("x".into()), "internal"),
            (PhenoError::Platform("x".into()), "platform"),
        ];
        for (err, expected_substr) in cases {
            assert!(
                err.to_string().contains(expected_substr),
                "expected '{}' in '{}'",
                expected_substr,
                err
            );
        }
    }

    #[test]
    fn timeout_display_and_retryable() {
        let e = PhenoError::Timeout;
        assert_eq!(e.to_string(), "timeout");
        assert!(e.is_retryable());
        assert!(!e.is_client_error());
    }

    #[test]
    fn unsupported_platform_display_and_code() {
        let e = PhenoError::unsupported_platform("CODE_A", "msg here");
        assert_eq!(e.status_code(), 501);
        assert_eq!(e.unsupported_code(), Some("CODE_A"));
        assert!(!e.is_client_error());
        assert!(!e.is_retryable());
        let s = e.to_string();
        assert!(s.contains("CODE_A"));
        assert!(s.contains("msg here"));
    }

    #[test]
    fn status_code_mapping() {
        assert_eq!(PhenoError::BadRequest("".into()).status_code(), 400);
        assert_eq!(PhenoError::Forbidden("".into()).status_code(), 403);
        assert_eq!(PhenoError::NotFound("".into()).status_code(), 404);
        assert_eq!(PhenoError::Timeout.status_code(), 504);
        assert_eq!(PhenoError::Internal("".into()).status_code(), 500);
        assert_eq!(PhenoError::Platform("".into()).status_code(), 500);
    }

    #[test]
    fn clone_preserves_equality() {
        let e = PhenoError::BadRequest("clone me".into());
        let cloned = e.clone();
        assert_eq!(e, cloned);
    }

    #[test]
    fn unsupported_platform_not_client_error() {
        let e = PhenoError::unsupported_platform("X", "Y");
        assert!(!e.is_client_error());
    }

    #[test]
    fn internal_not_retryable() {
        let e = PhenoError::Internal("oops".into());
        assert!(!e.is_retryable());
    }

    #[test]
    fn platform_not_retryable() {
        let e = PhenoError::Platform("driver".into());
        assert!(!e.is_retryable());
    }
}

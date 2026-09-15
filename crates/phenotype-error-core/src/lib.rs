//! Minimal `phenotype-error-core` stub for the Eidolon workspace.
//!
//! Provides [`ApiError`], the canonical cross-crate error type used by
//! `eidolon-core` and its downstream crates.

use thiserror::Error;

/// Canonical cross-crate API error type.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ApiError {
    /// Client sent a malformed or otherwise invalid request.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// Caller is not allowed to perform this operation.
    #[error("forbidden: {0}")]
    Forbidden(String),

    /// Operation did not complete within the allowed time.
    #[error("timeout")]
    Timeout,

    /// The requested resource was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// An internal error occurred.
    #[error("internal: {0}")]
    Internal(String),

    /// A platform-specific (OS / framework / driver) error.
    #[error("platform: {0}")]
    Platform(String),

    /// The requested OS / driver path is not implemented yet (fail-loud stub).
    ///
    /// Prefer this over [`Self::Internal`] when a platform driver is intentionally
    /// stubbed. `code` is a stable machine-readable token (see crate docs /
    /// `eidolon-desktop::codes`).
    #[error("unsupported platform [{code}]: {message}")]
    UnsupportedPlatform {
        /// Stable error code (e.g. `EIDOLON_DESKTOP_WIN_STUB`).
        code: String,
        /// Human-readable detail including method / extraction pointer.
        message: String,
    },
}

impl ApiError {
    /// Construct an [`Self::UnsupportedPlatform`] error.
    pub fn unsupported_platform(
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::UnsupportedPlatform {
            code: code.into(),
            message: message.into(),
        }
    }

    /// Returns `true` if this is a client-side (4xx-equivalent) error.
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::BadRequest(_) | Self::Forbidden(_) | Self::NotFound(_)
        )
    }

    /// Returns `true` if this error is transient and may be retried.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Timeout)
    }

    /// Returns an HTTP-like status code for this error.
    pub fn status_code(&self) -> u16 {
        match self {
            Self::BadRequest(_) => 400,
            Self::Forbidden(_) => 403,
            Self::NotFound(_) => 404,
            Self::Timeout => 504,
            // 501 Not Implemented — driver path exists but is intentionally stubbed.
            Self::UnsupportedPlatform { .. } => 501,
            Self::Internal(_) | Self::Platform(_) => 500,
        }
    }

    /// Machine-readable code when this is [`Self::UnsupportedPlatform`].
    pub fn unsupported_code(&self) -> Option<&str> {
        match self {
            Self::UnsupportedPlatform { code, .. } => Some(code.as_str()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_request_display() {
        let e = ApiError::BadRequest("missing field".into());
        assert!(e.to_string().contains("missing field"));
        assert!(e.is_client_error());
        assert!(!e.is_retryable());
    }

    #[test]
    fn forbidden_display() {
        let e = ApiError::Forbidden("access denied".into());
        assert!(e.to_string().contains("forbidden"));
        assert!(e.is_client_error());
    }

    #[test]
    fn timeout_is_retryable() {
        let e = ApiError::Timeout;
        assert!(e.is_retryable());
        assert!(!e.is_client_error());
    }

    #[test]
    fn not_found_display() {
        let e = ApiError::NotFound("resource/42".into());
        assert!(e.to_string().contains("not found"));
        assert!(e.is_client_error());
    }

    #[test]
    fn internal_display() {
        let e = ApiError::Internal("db panic".into());
        assert!(e.to_string().contains("internal"));
        assert!(!e.is_client_error());
    }

    #[test]
    fn platform_display() {
        let e = ApiError::Platform("CGEventSource failed".into());
        assert!(e.to_string().contains("platform"));
        assert!(!e.is_client_error());
        assert!(!e.is_retryable());
    }

    #[test]
    fn unsupported_platform_is_501_and_not_retryable() {
        let e = ApiError::unsupported_platform(
            "EIDOLON_DESKTOP_WIN_STUB",
            "SendInput not wired",
        );
        assert_eq!(e.status_code(), 501);
        assert!(!e.is_client_error());
        assert!(!e.is_retryable());
        assert_eq!(e.unsupported_code(), Some("EIDOLON_DESKTOP_WIN_STUB"));
        let s = e.to_string();
        assert!(s.contains("EIDOLON_DESKTOP_WIN_STUB"));
        assert!(s.contains("SendInput not wired"));
    }
}

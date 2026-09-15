//! `StageError` — typed error enum for `VirtualStage` operations.
//!
//! The default `VirtualStage` methods return `Result<T, PhenoError>` (the
//! alias re-exported from [`phenotype_errors`]). The `try_*` sibling
//! methods on the same traits return `StageResult<T> = Result<T,
//! StageError>`, where [`StageError`] is a stage-specific variant that
//! names *which* surface the failure came from (viewport query, capture,
//! input dispatch, exec, etc.).
//!
//! The per-method `try_*` defaults map the underlying [`PhenoError`]
//! into the appropriate [`StageError`] variant — `try_get_viewport`
//! produces `StageError::Viewport`, `try_exec` produces
//! `StageError::Exec`, etc. — so a caller can match on the failure mode
//! without re-parsing an opaque `Display` string.
//!
//! A blanket [`From<PhenoError>`] impl collapses any [`PhenoError`]
//! into [`StageError::Other`] for callers that just want a uniform
//! conversion without method-level specificity.

use std::fmt;

use crate::error::PhenoError;

/// Failure variants for `VirtualStage` / `MobileStage` / `SandboxStage`
/// operations.
///
/// Each variant corresponds to one or more trait methods:
/// - [`StageError::Viewport`] — `get_viewport`
/// - [`StageError::Capture`] — `screenshot`
/// - [`StageError::Input`] — `pointer`, `text`, `tap`, `swipe`,
///   `input_text`
/// - [`StageError::Record`] — `record_event`
/// - [`StageError::Metadata`] — `get_metadata`
/// - [`StageError::Lifecycle`] — `start`, `stop`
/// - [`StageError::Exec`] — `exec`
/// - [`StageError::Resource`] — `resource_usage`
/// - [`StageError::Other`] — catch-all (used by the blanket
///   `From<PhenoError>` impl and by callers that prefer a uniform
///   mapping).
#[derive(Debug)]
pub enum StageError {
    /// `get_viewport` failed.
    Viewport(String),
    /// `screenshot` failed.
    Capture(String),
    /// A pointer / text / tap / swipe / `input_text` call failed.
    Input(String),
    /// `record_event` failed.
    Record(String),
    /// `get_metadata` failed.
    Metadata(String),
    /// `start` / `stop` failed.
    Lifecycle(String),
    /// `exec` failed.
    Exec(String),
    /// `resource_usage` failed.
    Resource(String),
    /// Catch-all for any unclassified `PhenoError` mapping.
    Other(String),
}

impl StageError {
    /// Short, lowercase label for the variant — used by [`Display`].
    fn label(&self) -> &'static str {
        match self {
            Self::Viewport(_) => "viewport",
            Self::Capture(_) => "capture",
            Self::Input(_) => "input",
            Self::Record(_) => "record",
            Self::Metadata(_) => "metadata",
            Self::Lifecycle(_) => "lifecycle",
            Self::Exec(_) => "exec",
            Self::Resource(_) => "resource",
            Self::Other(_) => "other",
        }
    }

    /// Borrow the inner message string.
    pub fn message(&self) -> &str {
        match self {
            Self::Viewport(s)
            | Self::Capture(s)
            | Self::Input(s)
            | Self::Record(s)
            | Self::Metadata(s)
            | Self::Lifecycle(s)
            | Self::Exec(s)
            | Self::Resource(s)
            | Self::Other(s) => s,
        }
    }
}

impl fmt::Display for StageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "stage {} error: {}", self.label(), self.message())
    }
}

impl std::error::Error for StageError {}

/// Convenience result alias for stage operations that surface a
/// [`StageError`].
pub type StageResult<T> = std::result::Result<T, StageError>;

/// Blanket mapping from any [`PhenoError`] into [`StageError::Other`].
///
/// The per-method `try_*` defaults do *not* use this impl — they map
/// into the method-specific variant via a closure (e.g.
/// `try_get_viewport` produces `StageError::Viewport`). This impl is
/// here for callers (and tests) that want a uniform, no-method-context
/// conversion path.
impl From<PhenoError> for StageError {
    fn from(err: PhenoError) -> Self {
        Self::Other(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_variant_and_message() {
        let err = StageError::Viewport("no display".into());
        let rendered = err.to_string();
        assert!(rendered.contains("viewport"), "rendered = {rendered}");
        assert!(rendered.contains("no display"), "rendered = {rendered}");
    }

    #[test]
    fn from_pheno_error_goes_to_other_variant() {
        // `PhenoError` is an alias for `ApiError`; use the cheapest
        // variant (`Timeout`) since the From impl only reads
        // `Display`.
        let pheno: PhenoError = PhenoError::Timeout;
        let stage: StageError = pheno.into();
        assert!(matches!(stage, StageError::Other(_)));
    }

    #[test]
    fn stage_result_alias_is_consistent() {
        let err: StageResult<u32> = Err(StageError::Other("x".into()));
        assert!(err.is_err());
    }

    // -- backfill: exercise every variant so label() and message() branches
    //    are fully covered and the Display format is verified for each.

    #[test]
    fn display_capture_variant() {
        let err = StageError::Capture("screenshot failed".into());
        let s = err.to_string();
        assert!(s.contains("capture"), "label missing: {s}");
        assert!(s.contains("screenshot failed"), "message missing: {s}");
        assert_eq!(err.message(), "screenshot failed");
    }

    #[test]
    fn display_input_variant() {
        let err = StageError::Input("tap rejected".into());
        let s = err.to_string();
        assert!(s.contains("input"), "label missing: {s}");
        assert!(s.contains("tap rejected"), "message missing: {s}");
        assert_eq!(err.message(), "tap rejected");
    }

    #[test]
    fn display_record_variant() {
        let err = StageError::Record("audit write failed".into());
        let s = err.to_string();
        assert!(s.contains("record"), "label missing: {s}");
        assert_eq!(err.message(), "audit write failed");
    }

    #[test]
    fn display_metadata_variant() {
        let err = StageError::Metadata("no metadata".into());
        let s = err.to_string();
        assert!(s.contains("metadata"), "label missing: {s}");
        assert_eq!(err.message(), "no metadata");
    }

    #[test]
    fn display_lifecycle_variant() {
        let err = StageError::Lifecycle("start timed out".into());
        let s = err.to_string();
        assert!(s.contains("lifecycle"), "label missing: {s}");
        assert_eq!(err.message(), "start timed out");
    }

    #[test]
    fn display_exec_variant() {
        let err = StageError::Exec("permission denied".into());
        let s = err.to_string();
        assert!(s.contains("exec"), "label missing: {s}");
        assert_eq!(err.message(), "permission denied");
    }

    #[test]
    fn display_resource_variant() {
        let err = StageError::Resource("cgroup read failed".into());
        let s = err.to_string();
        assert!(s.contains("resource"), "label missing: {s}");
        assert_eq!(err.message(), "cgroup read failed");
    }

    #[test]
    fn display_other_variant() {
        let err = StageError::Other("unexpected".into());
        let s = err.to_string();
        assert!(s.contains("other"), "label missing: {s}");
        assert_eq!(err.message(), "unexpected");
    }

    #[test]
    fn stage_error_is_std_error() {
        // Verify std::error::Error is implemented and source chain is None
        // (no nested cause wrapping in this error type).
        let err: Box<dyn std::error::Error> = Box::new(StageError::Exec("cmd".into()));
        assert!(err.source().is_none());
    }

    #[test]
    fn stage_result_ok_arm() {
        let ok: StageResult<u32> = Ok(42);
        assert!(ok.is_ok());
    }

    #[test]
    fn message_viewport_round_trip() {
        let msg = "resolution unavailable";
        let err = StageError::Viewport(msg.into());
        assert_eq!(err.message(), msg);
    }
}

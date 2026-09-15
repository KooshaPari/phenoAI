//! Domain types for image analysis — zero external dependencies.

/// Result of a perceptual diff between two frames.
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// Whether the change ratio exceeded the requested threshold.
    pub changed: bool,
    /// Fraction of pixels that differ, in [0.0, 1.0].
    pub change_ratio: f64,
}

/// Result of a content hash operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashResult {
    /// Hex-encoded BLAKE3 hash of the normalized pixel data.
    pub hash: String,
}

/// Errors that can arise during image analysis.
#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("decode failed: {0}")]
    DecodeFailed(String),
    #[error("dimension mismatch: {0}x{1} vs {2}x{3}")]
    DimensionMismatch(u32, u32, u32, u32),
    #[error("hash failed: {0}")]
    HashFailed(String),
}

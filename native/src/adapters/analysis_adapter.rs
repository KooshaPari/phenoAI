//! NativeAnalysisAdapter — image analysis using image + blake3.
//! Implements AnalysisPort.

use crate::domain::analysis::{AnalysisError, DiffResult, HashResult};
use crate::ports::AnalysisPort;
use async_trait::async_trait;
use tracing::{debug, instrument};

pub struct NativeAnalysisAdapter;

impl NativeAnalysisAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NativeAnalysisAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AnalysisPort for NativeAnalysisAdapter {
    #[instrument(name = "analysis.diff", skip(self, a, b), fields(threshold = threshold))]
    async fn diff(&self, a: &[u8], b: &[u8], threshold: f32) -> Result<DiffResult, AnalysisError> {
        let a_bytes = a.to_vec();
        let b_bytes = b.to_vec();
        let threshold_f64 = threshold as f64;

        tokio::task::spawn_blocking(move || -> Result<DiffResult, AnalysisError> {
            let img_a = image::load_from_memory(&a_bytes)
                .map_err(|e| AnalysisError::DecodeFailed(format!("image_a: {e}")))?;
            let img_b = image::load_from_memory(&b_bytes)
                .map_err(|e| AnalysisError::DecodeFailed(format!("image_b: {e}")))?;

            let a_rgba = img_a.to_rgba8();
            let b_rgba = img_b.to_rgba8();

            if a_rgba.dimensions() != b_rgba.dimensions() {
                debug!("analysis.diff: dimension mismatch — fully changed");
                let (aw, ah) = a_rgba.dimensions();
                let (bw, bh) = b_rgba.dimensions();
                return Err(AnalysisError::DimensionMismatch(aw, ah, bw, bh));
            }

            let total_pixels = (a_rgba.width() * a_rgba.height()) as u64;
            let mut changed_pixels: u64 = 0;

            for (pa, pb) in a_rgba.pixels().zip(b_rgba.pixels()) {
                if pa.0[0] != pb.0[0] || pa.0[1] != pb.0[1] || pa.0[2] != pb.0[2] {
                    changed_pixels += 1;
                }
            }

            let change_ratio = changed_pixels as f64 / total_pixels as f64;
            debug!(
                "analysis.diff: {}/{} pixels changed ({:.2}%)",
                changed_pixels,
                total_pixels,
                change_ratio * 100.0
            );

            Ok(DiffResult {
                changed: change_ratio > threshold_f64,
                change_ratio,
            })
        })
        .await
        .map_err(|e| AnalysisError::DecodeFailed(format!("spawn_blocking panic: {e}")))?
    }

    #[instrument(name = "analysis.hash", skip(self, data), fields(len = data.len()))]
    async fn hash(&self, data: &[u8]) -> Result<HashResult, AnalysisError> {
        let bytes = data.to_vec();
        tokio::task::spawn_blocking(move || -> Result<HashResult, AnalysisError> {
            let img = image::load_from_memory(&bytes)
                .map_err(|e| AnalysisError::DecodeFailed(format!("image: {e}")))?;
            let rgba = img.to_rgba8();
            let raw_pixels = rgba.as_raw();
            let h = blake3::hash(raw_pixels);
            Ok(HashResult {
                hash: h.to_hex().to_string(),
            })
        })
        .await
        .map_err(|e| AnalysisError::HashFailed(format!("spawn_blocking panic: {e}")))?
    }
}

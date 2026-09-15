//! Image analysis: perceptual diff and BLAKE3 hashing.

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::debug;

// ---------------------------------------------------------------------------
// RPC handlers
// ---------------------------------------------------------------------------

pub async fn diff_rpc(params: Value) -> Result<Value> {
    #[derive(Deserialize)]
    struct DiffParams {
        image_a: String,
        image_b: String,
        threshold: Option<f64>,
    }
    let p: DiffParams = serde_json::from_value(params)?;
    let bytes_a = BASE64.decode(&p.image_a).context("Failed to decode image_a base64")?;
    let bytes_b = BASE64.decode(&p.image_b).context("Failed to decode image_b base64")?;
    let threshold = p.threshold.unwrap_or(0.02);
    let result = diff(&bytes_a, &bytes_b, threshold).await?;
    Ok(json!({
        "changed": result.changed,
        "change_ratio": result.change_ratio,
    }))
}

pub async fn hash_rpc(params: Value) -> Result<Value> {
    #[derive(Deserialize)]
    struct HashParams { image: String }
    let p: HashParams = serde_json::from_value(params)?;
    let bytes = BASE64.decode(&p.image).context("Failed to decode image base64")?;
    let h = hash(&bytes).await?;
    Ok(json!({ "hash": h }))
}

// ---------------------------------------------------------------------------
// Core implementations
// ---------------------------------------------------------------------------

pub struct DiffResult {
    pub changed: bool,
    pub change_ratio: f64,
}

/// Compute the fraction of pixels that differ between two PNG images.
/// `threshold` is the minimum change_ratio to consider the images "changed".
pub async fn diff(image_a_bytes: &[u8], image_b_bytes: &[u8], threshold: f64) -> Result<DiffResult> {
    let a_bytes = image_a_bytes.to_vec();
    let b_bytes = image_b_bytes.to_vec();
    tokio::task::spawn_blocking(move || -> Result<DiffResult> {
        let img_a = image::load_from_memory(&a_bytes).context("Failed to decode image_a")?;
        let img_b = image::load_from_memory(&b_bytes).context("Failed to decode image_b")?;

        let a_rgba = img_a.to_rgba8();
        let b_rgba = img_b.to_rgba8();

        // If dimensions differ, consider fully changed.
        if a_rgba.dimensions() != b_rgba.dimensions() {
            debug!("analysis.diff: dimension mismatch — fully changed");
            return Ok(DiffResult { changed: true, change_ratio: 1.0 });
        }

        let total_pixels = (a_rgba.width() * a_rgba.height()) as u64;
        let mut changed_pixels: u64 = 0;

        for (pa, pb) in a_rgba.pixels().zip(b_rgba.pixels()) {
            // Compare RGB channels; ignore alpha for visual change detection.
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
            changed: change_ratio > threshold,
            change_ratio,
        })
    })
    .await
    .context("spawn_blocking panicked")?
}

/// Compute a BLAKE3 hash of the raw pixel data of a PNG image.
/// Normalizes to RGBA8 before hashing so format differences don't matter.
pub async fn hash(image_bytes: &[u8]) -> Result<String> {
    let bytes = image_bytes.to_vec();
    tokio::task::spawn_blocking(move || -> Result<String> {
        let img = image::load_from_memory(&bytes).context("Failed to decode image for hashing")?;
        let rgba = img.to_rgba8();
        let raw_pixels = rgba.as_raw();
        let hash = blake3::hash(raw_pixels);
        Ok(hash.to_hex().to_string())
    })
    .await
    .context("spawn_blocking panicked")?
}

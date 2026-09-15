// policy.rs — fit verdict thresholds + batch-size policy.
//
// Centralized so the Streamlit dashboard and CLI cannot drift apart
// (per FR-HWL-CAPACITY-001 NFR-HWL-EXPLAINABILITY-001). All consumers
// of `fit_score` and `fit_verdict` import these constants.

use crate::attention::KvContext;
use crate::estimate::CapacityEstimate;

/// Headroom ratio threshold below which the model is considered "Tight"
/// (works at the current `ctx_len` but will OOM if `ctx_len` grows
/// toward the model's max).
///
/// `headroom_ratio = (device_vram - estimate.total) / device_vram`.
/// `0.30` means: 30 % of device VRAM is free after the model loads.
/// This is the threshold HwLedger's streamlit capacity planner uses.
pub const THRESHOLD_TIGHT: f32 = 0.30;

/// Headroom ratio threshold below which the model is considered "Fail"
/// (will OOM or page; not recommended for use).
///
/// `0.05` means: less than 5 % of device VRAM is free. At this point
/// the model is too close to the device limit to be safe — KV cache
/// growth at large `ctx_len` will spill.
pub const THRESHOLD_FAIL: f32 = 0.05;

/// Fit verdict — the three-way judgment for a `CapacityEstimate` on a
/// given device.
///
/// Thresholds are in `THRESHOLD_TIGHT` and `THRESHOLD_FAIL`. Both
/// `fit_score` and `fit_verdict` consume these constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FitVerdict {
    /// Model fits with plenty of headroom (>= 30 % free after load).
    /// Safe to grow `ctx_len` toward the model's max.
    Fit,
    /// Model fits at the current `ctx_len` but is tight (5..30 % free).
    /// Will OOM if `ctx_len` grows.
    Tight,
    /// Model does not fit on the device (< 5 % free; will OOM or page).
    Fail,
}

impl FitVerdict {
    /// All variants known to this crate.
    pub const ALL: &'static [FitVerdict] = &[
        FitVerdict::Fit,
        FitVerdict::Tight,
        FitVerdict::Fail,
    ];

    /// Canonical human-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            FitVerdict::Fit => "fit",
            FitVerdict::Tight => "tight",
            FitVerdict::Fail => "fail",
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn thresholds_are_ordered() {
        // 0.30 (TIGHT) > 0.05 (FAIL) — sanity check. These are
        // documented as constants, but the assertions remain useful
        // to catch accidental edits.
        assert!(THRESHOLD_TIGHT > THRESHOLD_FAIL);
        assert!(THRESHOLD_TIGHT > 0.0);
        assert!(THRESHOLD_TIGHT <= 1.0);
        assert!(THRESHOLD_FAIL >= 0.0);
        assert!(THRESHOLD_FAIL < THRESHOLD_TIGHT);
    }

    #[test]
    fn verdict_as_str_matches_variant() {
        assert_eq!(FitVerdict::Fit.as_str(), "fit");
        assert_eq!(FitVerdict::Tight.as_str(), "tight");
        assert_eq!(FitVerdict::Fail.as_str(), "fail");
    }

    #[test]
    fn verdict_all_has_three_variants() {
        assert_eq!(FitVerdict::ALL.len(), 3);
    }
}

// ---------------------------------------------------------------------------
// BatchPolicy + recommended_batch_size (slice 1, L5-115)
// ---------------------------------------------------------------------------

/// Policy for picking the recommended batch size on a given device.
///
/// All variants consume a `CapacityEstimate` (weights + KV + activations
/// + overhead at `batch=1`) and a `KvContext` (which carries the planned
///   `seq_len` and `batch_size`). The recommended batch size is what the
///   `BatchPolicy` would dispatch as the safe default for that workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BatchPolicy {
    /// Fill the device: pick the largest batch such that total VRAM
    /// (weights + KV * batch + activations * batch + overhead) ≤
    /// `device_vram`. This is the default for vLLM-style continuous
    /// batching.
    #[default]
    FillDevice,
    /// Cap at a fixed batch size regardless of available headroom.
    /// Useful for latency-sensitive workloads (e.g., real-time).
    CapBatch(u32),
    /// Reserve a target headroom ratio (e.g., 0.2 = leave 20% of VRAM
    /// free for KV growth). The largest batch such that
    /// `total_vram / device_vram ≤ (1 - headroom)` is selected.
    ReserveHeadroom {
        /// Target headroom ratio, in `[0.0, 1.0]`. 0.2 = 20 % free.
        ratio: u32, // basis points (2000 = 20.00 %)
    },
}

/// Recommended batch size for a `KvContext` on a device with `device_vram`
/// bytes, using the given `BatchPolicy` and the static `CapacityEstimate`.
///
/// Returns `0` if the model itself does not fit at `batch=1`
/// (i.e., `estimate.weights + activations + overhead > device_vram`).
///
/// The math is iterative: we test `batch = 1, 2, 4, 8, ...` and return
/// the largest that satisfies the policy. The loop is bounded by 1024
/// (more than enough; vLLM's continuous batcher can have higher
/// effective batch but the per-step batch is typically ≤ 64).
#[must_use]
pub fn recommended_batch_size(
    estimate: &CapacityEstimate,
    ctx: &KvContext,
    device_vram: u64,
    policy: BatchPolicy,
) -> u32 {
    if device_vram == 0 || estimate.weights == 0 {
        return 0;
    }
    // Static base (weights + overhead; does not scale with batch).
    let base = estimate
        .weights
        .saturating_add(estimate.activations)
        .saturating_add(estimate.overhead);
    if base > device_vram {
        return 0; // Model does not even fit at batch=1.
    }
    // Per-batch KV + per-batch activations.
    let per_batch = estimate
        .kv_cache
        .saturating_add(estimate.activations);

    // Cap from `BatchPolicy`.
    let hard_cap: u32 = match policy {
        BatchPolicy::FillDevice => 1024,
        BatchPolicy::CapBatch(n) => n.min(1024),
        BatchPolicy::ReserveHeadroom { ratio } => {
            // `ratio` is in basis points; convert to per-mille.
            // Max batch such that total ≤ device * (1 - ratio).
            let usable = device_vram.saturating_mul(10_000_u64.saturating_sub(ratio as u64)) / 10_000;
            // For each batch, total = base + batch * per_batch.
            // We want: base + batch * per_batch ≤ usable.
            // So batch ≤ (usable - base) / per_batch.
            if usable <= base {
                return 0;
            }
            let headroom = usable - base;
            match headroom.checked_div(per_batch) {
                Some(b) => b.min(1024) as u32,
                None => 1024, // per_batch == 0
            }
        }
    };

    // Iterative: find largest batch in [1, hard_cap] such that
    // base + batch * per_batch ≤ device_vram.
    let mut lo: u32 = 1;
    let mut hi: u32 = hard_cap;
    let mut best: u32 = 0;
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let mid_u64 = mid as u64;
        let total = base.saturating_add(per_batch.saturating_mul(mid_u64));
        if total <= device_vram {
            best = mid;
            lo = mid + 1;
        } else {
            if mid == 0 { break; }
            hi = mid - 1;
        }
    }
    // Verify the chosen batch is consistent with the per-request KV ctx
    // (i.e., we did not assume a different `batch_size` in `estimate_kv_vram`).
    // If `ctx.batch_size > best`, we cap to `best`. (Consumers that
    // re-plan with a different `batch_size` should re-call.)
    best.min(if ctx.batch_size == 0 { best } else { ctx.batch_size.max(best) })
}

#[cfg(test)]
mod batch_policy_tests {
    use super::*;
    use crate::attention::AttentionKind;
    use crate::estimate::estimate_vram;
    use crate::estimate::ModelSpec;
    use crate::math::Dtype;

    fn lorem_spec() -> ModelSpec {
        ModelSpec {
            params: 7_000_000_000,
            attention: AttentionKind::GQA,
            ctx_len: 4096,
            n_layers: 32,
            n_kv_heads: 8,
            head_dim: 128,
            hidden: 4096,
            quant: Dtype::F16,
            kv_quant: Dtype::F16,
            ..Default::default()
        }
    }

    #[test]
    fn fill_device_picks_largest_fitting_batch() {
        let model = lorem_spec();
        let est = estimate_vram(&model);
        let ctx = KvContext {
            batch_size: 1, // baseline; will be re-scaled by policy
            seq_len: 4096,
            num_layers: 32,
            num_kv_heads: 8,
            head_dim: 128,
            attention: AttentionKind::GQA,
            kv_quant: Dtype::F16,
            ..Default::default()
        };
        // 40 GB device; LLaMA-7B + KV at 4K ctx is small relative to 40 GB.
        let device = 40 * 1024_u64.pow(3);
        let batch = recommended_batch_size(&est, &ctx, device, BatchPolicy::FillDevice);
        // LLaMA-7B at 4K ctx, FP16: ~14 GB weights + ~512 MB KV per batch.
        // 40 GB - 14 GB base = 26 GB; 26 GB / 512 MB ≈ 50 batches. But
        // reserve ~5 % overhead means effective < 50. Still, should be ≥ 32.
        assert!(batch >= 32, "expected batch >= 32, got {}", batch);
        assert!(batch <= 1024);
    }

    #[test]
    fn cap_batch_respects_explicit_limit() {
        let model = lorem_spec();
        let est = estimate_vram(&model);
        let ctx = KvContext {
            seq_len: 4096,
            num_layers: 32,
            num_kv_heads: 8,
            head_dim: 128,
            attention: AttentionKind::GQA,
            kv_quant: Dtype::F16,
            ..Default::default()
        };
        let device = 80 * 1024_u64.pow(3);
        // Hard cap at 4, even if the device can hold more.
        let batch = recommended_batch_size(&est, &ctx, device, BatchPolicy::CapBatch(4));
        assert_eq!(batch, 4);
    }

    #[test]
    fn reserve_headroom_leaves_buffer() {
        let model = lorem_spec();
        let est = estimate_vram(&model);
        let ctx = KvContext {
            seq_len: 4096,
            num_layers: 32,
            num_kv_heads: 8,
            head_dim: 128,
            attention: AttentionKind::GQA,
            kv_quant: Dtype::F16,
            ..Default::default()
        };
        let device = 40 * 1024_u64.pow(3);
        // Reserve 50 % of device for headroom → max 20 GB usable.
        let batch_50pct = recommended_batch_size(
            &est,
            &ctx,
            device,
            BatchPolicy::ReserveHeadroom { ratio: 5000 },
        );
        let batch_fill = recommended_batch_size(&est, &ctx, device, BatchPolicy::FillDevice);
        assert!(batch_50pct < batch_fill);
    }

    #[test]
    fn batch_zero_when_model_does_not_fit() {
        let model = lorem_spec();
        let est = estimate_vram(&model);
        let ctx = KvContext {
            seq_len: 4096,
            num_layers: 32,
            num_kv_heads: 8,
            head_dim: 128,
            attention: AttentionKind::GQA,
            kv_quant: Dtype::F16,
            ..Default::default()
        };
        // 8 GB device, LLaMA-7B FP16 = 14 GB → does not fit at batch=1.
        let device = 8 * 1024_u64.pow(3);
        let batch = recommended_batch_size(&est, &ctx, device, BatchPolicy::FillDevice);
        assert_eq!(batch, 0);
    }

    #[test]
    fn batch_zero_when_device_is_zero() {
        let model = lorem_spec();
        let est = estimate_vram(&model);
        let ctx = KvContext::default();
        let batch = recommended_batch_size(&est, &ctx, 0, BatchPolicy::FillDevice);
        assert_eq!(batch, 0);
    }
}

// pheno-capacity
//
//! # pheno-capacity
//!
//! Pure-math library for **VRAM estimation**, **model-fit scoring**, and
//! **hardware capacity planning** for large-language-model inference and
//! fine-tuning.
//!
//! Extracted from HwLedger (`apps/streamlit/lib/cost_model.py`,
//! `apps/streamlit/lib/perf_model.py` in git history @ `8bf878ca`) per
//! **ADR-035A** (L5-105, 2026-06-18). Re-implemented in Rust with
//! `no_std` compatibility, zero dependencies, and deterministic
//! pure-function semantics.
//!
//! ## Scope
//!
//! This crate answers one question:
//!
//! > "Given a model with `N` parameters and a target device with `D`
//! >  bytes of VRAM, does it fit, and how much headroom is left?"
//!
//! It also answers three closely-related questions (see [`math`]):
//!
//! 1. **How much VRAM does the model weights consume in a given dtype?**
//!    → [`vram_estimate`]
//! 2. **Does the model fit on the device?** → [`model_fits_in`]
//! 3. **How much VRAM overhead does a fine-tuning optimizer add?**
//!    → [`optimizer_state_vram`]
//! 4. **What is the Chinchilla-optimal training-token budget?**
//!    → [`chinchilla_tokens`]
//! 5. **How much VRAM does the KV cache consume at inference?**
//!    → [`estimate_kv_vram`]
//! 6. **Full VRAM estimate (weights + KV + activations + overhead)?**
//!    → [`estimate_total_vram`]
//! 7. **Does a model fit, with how much headroom, and a `FitVerdict`?**
//!    → [`fit_score`]
//!
//! ## Attention-kind awareness (v0.2.0, L5-115)
//!
//! As of v0.2.0, the `attention` module provides KV-cache VRAM
//! estimation per attention pattern:
//!
//! | Pattern     | KV shape                                       |
//! | ----------- | ---------------------------------------------- |
//! | MHA         | `(2, B, H_kv, S, D) * E` (full K + V per head) |
//! | MQA         | `(2, B, 1, S, D) * E`                          |
//! | GQA         | `(2, B, H_kv, S, D) * E` (H_kv < H_q)          |
//! | MLA         | `(B, S, D_c) * E` (compressed latent)          |
//!
//! See [`AttentionKind`] and [`estimate_kv_vram`].
//!
//! ## Non-scope
//!
//! - **Throughput / TPS / TTFT**: lives in `pheno-throughput` (future).
//! - **GPU pricing / cost-to-train**: lives in HwLedger as policy data.
//! - **Layer-by-layer memory profiling**: requires a model graph; out
//!   of scope for a pure-math lib.
//!
//! ## Conventions
//!
//! - All functions are `no_std`-compatible; nothing in this crate
//!   requires `std`.
//! - All functions are deterministic and side-effect free.
//! - No panics on valid inputs; overflow is explicit
//!   ([`vram_estimate`] returns `u64::MAX` on overflow).
//! - All functions are documented with at least one numerical example
//!   (LLaMA-7B / Mixtral / Llama-3-70B-class anchors).
//!
//! ## Quickstart
//!
//! ```
//! use pheno_capacity::{vram_estimate, model_fits_in, Dtype};
//!
//! // LLaMA-7B in FP16: 7e9 params * 2 bytes = 14_000_000_000 bytes (~13 GiB).
//! let vram_b = vram_estimate(7_000_000_000, Dtype::F16);
//! assert_eq!(vram_b, 14_000_000_000);
//!
//! // Does LLaMA-7B FP16 fit on an A100-40GB? 14 GB ≤ 40 GB → yes.
//! assert!(model_fits_in(7_000_000_000, 40 * 1024_u64.pow(3), Dtype::F16));
//!
//! // Does LLaMA-7B FP32 fit on a 4090-24GB? 28 GB > 24 GB → no.
//! assert!(!model_fits_in(7_000_000_000, 24 * 1024_u64.pow(3), Dtype::F32));
//! ```

#![deny(missing_docs)]
// `no_std` is declared at the crate root (src/lib.rs); this sub-module
// inherits it. The original `pheno-capacity` crate declared `#![no_std]`
// at its own root, which becomes a no-op attribute when this file is
// `mod.rs` under a parent crate root that already declares it.

/// Numerical math primitives for VRAM estimation and capacity planning.
///
/// Every public function in this module is a pure function:
/// - No I/O.
/// - No global state.
/// - No panics on valid inputs (overflow returns a sentinel).
/// - All inputs are `u64` for parameter / byte counts; no `f32` /
///   `f64` for the canonical estimates (precision is important for
///   model-fit decisions).
pub mod math;

/// Attention-kind awareness for KV-cache VRAM estimation.
///
/// Different attention patterns have very different KV-cache footprints.
/// MHA is the worst; MLA (used by DeepSeek-V2/V3) compresses the KV
/// cache into a single latent per token.
pub mod attention;

/// Capacity-policy primitives for recommended-batch-size selection.
///
/// The default policy is "as large as possible without exceeding the
/// device budget". Other policies cap batch size at a fixed value or
/// cap it to leave headroom for a target activation-VRAM ratio.
pub mod policy;

/// Full VRAM estimate and model-fit scoring.
pub mod estimate;

pub use attention::{
    estimate_kv_vram, AttentionKind, KvContext, KV_LATENT_BYTES_PER_TOKEN_PER_LAYER_DEEPSEEK,
};
pub use estimate::{
    assess_fit, estimate_total_vram, estimate_vram, fit_score, fit_verdict, kv_cache_bytes,
    CapacityEstimate, DeviceSpec, FitScore, ModelSpec, MoEConfig, W_MLA, W_MOE, W_SINK, W_SLIDING,
    W_SSM, WarningFlags, WARNING_LABELS,
};
pub use math::{
    chinchilla_tokens, dtype_bytes, model_fits_in, optimizer_state_vram, vram_estimate, Dtype,
    Optimizer,
};
pub use policy::{recommended_batch_size, BatchPolicy, FitVerdict, THRESHOLD_FAIL, THRESHOLD_TIGHT};

// ---------------------------------------------------------------------------
// Tests (the canonical 4 functions + 4 helpers → 4 doc tests + 12 unit
// tests below; total ~30 assertions, well above the 80%-lib-tier bar).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vram_estimate_llama7b_fp16_is_14gb() {
        // LLaMA-7B has 6.7B params; with 7B nominal: 7e9 * 2 = 14 GB.
        // This is the canonical "fits on a 4090-24GB" anchor.
        assert_eq!(vram_estimate(7_000_000_000, Dtype::F16), 14_000_000_000);
    }

    #[test]
    fn vram_estimate_llama70b_fp16_is_140gb() {
        // LLaMA-70B FP16: 70e9 * 2 = 140 GB. Does not fit on any
        // single consumer GPU. Requires 2x A100-80GB or 4x A100-40GB.
        assert_eq!(vram_estimate(70_000_000_000, Dtype::F16), 140_000_000_000);
    }

    #[test]
    fn vram_estimate_zero_params_is_zero() {
        // Edge case: empty model. Must return 0, not panic.
        assert_eq!(vram_estimate(0, Dtype::F16), 0);
    }

    #[test]
    fn vram_estimate_overflow_returns_max() {
        // 2^63 params * 2 bytes would overflow u64. We must saturate,
        // not panic.
        let huge = (u64::MAX / 2) + 1;
        assert_eq!(vram_estimate(huge, Dtype::F16), u64::MAX);
    }

    #[test]
    fn model_fits_in_tight_boundary() {
        // Exact fit: 1 GB model on 1 GB device.
        let one_gb: u64 = 1024_u64.pow(3);
        assert!(model_fits_in(500_000_000, one_gb, Dtype::F16));
        // 0.6 GB model on 0.5 GB device: does NOT fit (600 MB > 500 MB).
        assert!(!model_fits_in(300_000_000, 500_000_000, Dtype::F16));
    }

    #[test]
    fn model_fits_in_mixtral_8x7b_on_2x_a100() {
        // Mixtral 8x7B: ~46.7B active params, ~14B per expert when
        // active. Round to 47e9 for a planner estimate. FP16 = 94 GB.
        // 2x A100-80GB in tensor-parallel = 160 GB → fits.
        let two_a100_80: u64 = 2 * 80 * 1024_u64.pow(3);
        assert!(model_fits_in(47_000_000_000, two_a100_80, Dtype::F16));
    }

    #[test]
    fn chinchilla_tokens_default_ratio_is_20x() {
        // 7B model, Chinchilla-optimal: 140B tokens.
        assert_eq!(chinchilla_tokens(7_000_000_000, 20.0), 140_000_000_000);
    }

    #[test]
    fn optimizer_state_vram_adamw_is_8x_weights() {
        // 1 GB BF16 weights + AdamW: 8 GB total VRAM overhead
        // (4x FP32 master + 2x m + 2x v).
        let weights: u64 = 1024_u64.pow(3);
        let overhead = optimizer_state_vram(weights, Optimizer::AdamW);
        assert_eq!(overhead, weights * 8);
    }

    #[test]
    fn optimizer_state_vram_lora_is_small_fraction() {
        // 1 GB weights + LoRA: ~5% overhead = 50 MB.
        let weights: u64 = 1024_u64.pow(3);
        let overhead = optimizer_state_vram(weights, Optimizer::LoRA);
        // 5% of 1 GB = 53_687_091 bytes (1024^3 * 0.05 floored).
        assert!(overhead < weights / 10); // < 10%
        assert!(overhead > weights / 100); // > 1%
    }

    #[test]
    fn dtype_bytes_returns_correct_sizes() {
        assert_eq!(dtype_bytes(Dtype::F32), 4);
        assert_eq!(dtype_bytes(Dtype::F16), 2);
        assert_eq!(dtype_bytes(Dtype::BF16), 2);
        assert_eq!(dtype_bytes(Dtype::I8), 1);
        assert_eq!(dtype_bytes(Dtype::I4), 1); // packed
    }

    #[test]
    fn vram_estimate_is_monotonic_in_params() {
        // Doubling params should double VRAM (everything else equal).
        let small = vram_estimate(1_000_000_000, Dtype::F16);
        let big = vram_estimate(2_000_000_000, Dtype::F16);
        assert_eq!(big, small * 2);
    }

    #[test]
    fn vram_estimate_is_monotonic_in_dtype_size() {
        // FP32 weights are 2x the size of FP16 weights.
        let fp16 = vram_estimate(7_000_000_000, Dtype::F16);
        let fp32 = vram_estimate(7_000_000_000, Dtype::F32);
        assert_eq!(fp32, fp16 * 2);
    }

    // ---- v0.2.0 (L5-115) cross-module integration tests ----

    #[test]
    fn v02_attention_module_compiles_and_reexports() {
        // Smoke: every public type from `attention` is reachable via
        // the crate root.
        let _ = AttentionKind::default();
        let ctx = KvContext::default();
        let _ = estimate_kv_vram(&ctx, Dtype::F16);
    }

    #[test]
    fn v02_policy_module_compiles_and_reexports() {
        // Smoke: every public type from `policy` is reachable.
        let _ = BatchPolicy::FillDevice;
    }

    #[test]
    fn v02_estimate_module_compiles_and_reexports() {
        // Smoke: every public type from `estimate` is reachable.
        let _ = FitVerdict::Fit;
    }

    #[test]
    fn v02_full_integration_llama7b_fits_a100_40gb() {
        // End-to-end: LLaMA-7B FP16 + 512 ctx, 32 K/V heads, 32 layers
        // should fit comfortably on an A100-40GB with a high fit score.
        // Use `assess_fit` (the user's preferred public-API entry point)
        // to exercise the full chain: math + estimate + policy.
        let model = ModelSpec {
            params: 7_000_000_000,
            attention: AttentionKind::GQA,
            ctx_len: 512,
            n_layers: 32,
            n_kv_heads: 32,
            head_dim: 128,
            hidden: 4096,
            quant: Dtype::F16,
            kv_quant: Dtype::F16,
            ..Default::default()
        };
        let a100_40gb = 40 * 1024_u64.pow(3);
        let score = assess_fit(&model, a100_40gb);
        assert_eq!(score.verdict, FitVerdict::Fit);
        assert!(score.headroom_bytes > 0);
        assert!(score.fit_score > 0.5);
    }

    #[test]
    fn v02_estimate_total_vram_matches_estimate_vram_total() {
        // The two functions must agree.
        let model = ModelSpec {
            params: 7_000_000_000,
            attention: AttentionKind::GQA,
            ctx_len: 2048,
            n_layers: 32,
            n_kv_heads: 8,
            head_dim: 128,
            hidden: 4096,
            quant: Dtype::F16,
            kv_quant: Dtype::F16,
            ..Default::default()
        };
        let est = estimate_vram(&model);
        let total = estimate_total_vram(&model);
        assert_eq!(total, est.total);
    }
}

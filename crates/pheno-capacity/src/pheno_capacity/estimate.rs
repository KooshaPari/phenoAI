// estimate.rs — full VRAM estimate (weights + KV + activations + overhead),
// MoE modeling, and the `fit_score` / `fit_verdict` judgment.
//
// Layered on top of `math.rs` (which provides `vram_estimate` for weights
// only). This module is the architecture-keyed dispatch: it picks the
// correct KV formula based on `AttentionKind`, applies MoE active-param
// accounting, and assembles the full breakdown.
//
// References (see docs/methodology.md for full citation list):
//   - MoE active params: Mixtral (arXiv:2401.04088), DeepSeek-V3 (arXiv:2412.19437)
//   - Activation memory: standard transformer analysis (forward pass only;
//     training requires gradient scratch, out of scope here)
//   - 5% overhead: empirical vLLM / TGI CUDA context reserve
//
// `extern crate alloc` is conditional on the `alloc` feature so the
// core math remains `no_std`-compatible. The `warning_labels` function
// returns an `alloc::vec::Vec` and is gated on the same feature.
#[cfg(feature = "alloc")]
extern crate alloc;

use crate::attention::AttentionKind;
use crate::math::{dtype_bytes, vram_estimate, Dtype};
use crate::policy::{FitVerdict, THRESHOLD_FAIL, THRESHOLD_TIGHT};

/// MoE (Mixture-of-Experts) configuration.
///
/// For non-MoE models, leave `ModelSpec.moe = None` and the full
/// `ModelSpec.params` count is used for VRAM purposes.
///
/// For MoE models, the active (forward-pass) parameter count is:
///
/// ```text
/// active_params = shared_params + (active_experts * expert_params)
/// ```
///
/// `shared_params` is the always-resident attention + embedding + LM
/// head; `expert_params` is per-expert. For Mixtral-8x7B:
/// `shared_params ≈ 14 B`, `expert_params ≈ 4.25 B` (×8 total, 2
/// active per token). The **active** count is what matters for VRAM
/// during inference; the **total** (46.7 B for Mixtral) is what matters
/// for disk size and full fine-tuning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoEConfig {
    /// Total number of experts in the MoE layer (e.g. 8 for Mixtral-8x7B).
    pub total_experts: u32,
    /// Number of experts active per token (e.g. 2 for Mixtral-8x7B).
    pub active_experts: u32,
    /// Parameters per expert (e.g. ~4.25B for Mixtral-8x7B per expert).
    pub expert_params: u64,
    /// Always-resident params (attention + embedding + LM head).
    /// Set to 0 if you want `active_params` to be the full `params` minus
    /// the per-expert portion (alternative accounting).
    pub shared_params: u64,
}

impl MoEConfig {
    /// Compute the active parameter count for VRAM purposes.
    /// Saturates to `u64::MAX` on overflow.
    #[must_use]
    pub fn active_params(&self) -> u64 {
        let experts = (self.active_experts as u64).saturating_mul(self.expert_params);
        self.shared_params.saturating_add(experts)
    }
}

/// Model specification for the architecture-keyed VRAM estimate.
///
/// Field-by-field, the relevant fields for each `AttentionKind`:
///
/// | Field           | MHA/MQA/GQA | MLA | SLIDING | SSM | HYBRID | SINK |
/// |-----------------|-------------|-----|---------|-----|--------|------|
/// | `params`        | total params (or shared for MoE) | same | same | same | same | same |
/// | `n_kv_heads`    | yes         | no  | yes     | no  | yes (for attn blocks) | yes |
/// | `head_dim`      | yes         | no  | yes     | no  | yes (for attn blocks) | yes |
/// | `kv_latent_dim` | no          | yes | no      | no  | no     | no  |
/// | `window_size`   | no          | no  | yes     | no  | no     | yes |
/// | `sink_tokens`   | no          | no  | no      | no  | no     | yes |
/// | `state_dim`     | no          | no  | no      | yes | yes (for SSM blocks) | no |
/// | `n_attn_layers` | no          | no  | no      | no  | yes   | no  |
/// | `hidden`        | yes (for activations) | same | same | same | same | same |
///
/// Fields not relevant to a given attention kind should be set to 0
/// (the `Default` impl does this).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModelSpec {
    /// Total parameters (or `shared_params` if `moe` is `Some`).
    /// Note: when `moe` is `Some`, this field stores `shared_params`
    /// and the per-expert count lives in `MoEConfig.expert_params`.
    pub params: u64,
    /// Attention architecture. Dispatches the KV formula.
    pub attention: AttentionKind,
    /// Current context length in tokens. Used for linear-in-`ctx_len`
    /// attention kinds (MHA/MQA/GQA/MLA/HYBRID). For constant kinds
    /// (SSM/SLIDING/SINK), this is informational and the cache size is
    /// bounded by `window_size` / `state_dim`.
    pub ctx_len: u32,
    /// Number of transformer blocks.
    pub n_layers: u32,
    /// Number of K/V heads. For MQA this should be 1.
    /// For SSM, set to 0.
    pub n_kv_heads: u32,
    /// Per-head dimension (e.g. 128 for LLaMA, 64 for many GQA models).
    /// For SSM, set to 0.
    pub head_dim: u32,
    /// Hidden size (e.g. 4096 for LLaMA-7B). Used for activation memory.
    pub hidden: u32,
    /// MoE configuration; `None` for dense models.
    pub moe: Option<MoEConfig>,
    /// Weight quantization dtype.
    pub quant: Dtype,
    /// KV cache quantization dtype. Typically `Dtype::F16` or
    /// `Dtype::BF16`; some inference engines use `Dtype::I8` (KIVI,
    /// KVQuant). Defaults to `Dtype::F16`.
    pub kv_quant: Dtype,
    /// MLA only: low-rank latent dimension for compressed K/V.
    /// Typically `head_dim / 4` for DeepSeek-V2. Set to 0 for non-MLA.
    pub kv_latent_dim: u32,
    /// SLIDING / SINK only: window size in tokens.
    /// Mistral 7B: 4096. Set to 0 for non-SLIDING/non-SINK.
    pub window_size: u32,
    /// SINK only: number of attention-sink tokens (StreamingLLM).
    /// Typically 4. Set to 0 for non-SINK.
    pub sink_tokens: u32,
    /// SSM only: state-space state dimension. Mamba: 16. Mamba-2: 64-128.
    /// Set to 0 for non-SSM.
    pub state_dim: u32,
    /// HYBRID only: number of attention blocks (vs SSM blocks). If 0,
    /// defaults to `n_layers` (i.e., all blocks are attention — the
    /// HYBRID formula reduces to GQA in that case).
    pub n_attn_layers: u32,
}

impl Default for ModelSpec {
    fn default() -> Self {
        Self {
            params: 0,
            attention: AttentionKind::GQA,
            ctx_len: 0,
            n_layers: 0,
            n_kv_heads: 0,
            head_dim: 0,
            hidden: 0,
            moe: None,
            quant: Dtype::F16,
            kv_quant: Dtype::F16,
            kv_latent_dim: 0,
            window_size: 0,
            sink_tokens: 0,
            state_dim: 0,
            n_attn_layers: 0,
        }
    }
}

/// Device specification for the fit judgment.
///
/// Minimal: just total VRAM. The `name` field is for display in
/// dashboards / CLI output; it is not used by the math.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceSpec {
    /// Total VRAM in bytes (e.g. `40 * 1024_u64.pow(3)` for A100-40GB).
    pub vram_bytes: u64,
    /// Display name (e.g. `"A100-40GB"`). Optional; `""` if unused.
    pub name: &'static str,
}

/// Bitfield of under-specified-input warnings.
///
/// Each bit corresponds to a known warning condition. Use
/// `has_warning(W_MLA)` etc. to test, or `warning_labels()` to get the
/// canonical human-readable list. Storing as `u8` keeps
/// `CapacityEstimate` `Copy` and `no_std`-friendly.
pub type WarningFlags = u8;

/// Bit: MLA specified but `kv_latent_dim` is 0.
pub const W_MLA: WarningFlags = 1 << 0;
/// Bit: SLIDING specified but `window_size` is 0.
pub const W_SLIDING: WarningFlags = 1 << 1;
/// Bit: SINK specified but `sink_tokens` or `window_size` is 0.
pub const W_SINK: WarningFlags = 1 << 2;
/// Bit: SSM specified but `state_dim` is 0.
pub const W_SSM: WarningFlags = 1 << 3;
/// Bit: MoE `active_experts` is 0 or greater than `total_experts`.
pub const W_MOE: WarningFlags = 1 << 4;

/// Full VRAM estimate: per-axis breakdown plus aggregated total.
///
/// All byte counts are `u64`. The `total` field is the sum of
/// `weights + kv_cache + activations + overhead` (saturated to
/// `u64::MAX` on overflow).
///
/// `warning_flags` is a bitfield of under-specified-input conditions
/// (see `W_MLA`, `W_SLIDING`, etc.). Use `has_warning(flag)` to test,
/// or `warning_labels()` to get the canonical list. Bitfield is
/// `no_std`-friendly and keeps `CapacityEstimate` `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CapacityEstimate {
    /// Weight bytes (post-quant; MoE-aware via `MoEConfig.active_params`).
    pub weights: u64,
    /// KV cache bytes (per-attention-kind formula).
    pub kv_cache: u64,
    /// Activation scratch bytes (forward-pass only; conservative).
    pub activations: u64,
    /// Framework + CUDA context reserve (`max(weights * 0.05, 256 MiB)`).
    pub overhead: u64,
    /// Sum of the above (saturated).
    pub total: u64,
    /// Bitfield of under-specified-input warnings. See `W_*` constants.
    pub warning_flags: WarningFlags,
}

impl CapacityEstimate {
    /// `true` if the given warning bit is set.
    #[must_use]
    pub const fn has_warning(&self, flag: WarningFlags) -> bool {
        (self.warning_flags & flag) != 0
    }

    /// `true` if any warning is set.
    #[must_use]
    pub const fn has_any_warning(&self) -> bool {
        self.warning_flags != 0
    }
}

/// Canonical ordered list of warning labels, indexed by bit position
/// (LSB → MSB = W_MLA → W_MOE). The slice is `&'static` so it is
/// `no_std`-friendly (no allocation).
pub const WARNING_LABELS: &[&str] = &[
    "MLA specified but kv_latent_dim is 0; result equals zero KV", // W_MLA
    "SLIDING specified but window_size is 0; KV cache collapses to 0", // W_SLIDING
    "SINK specified but sink_tokens or window_size is 0",          // W_SINK
    "SSM specified but state_dim is 0; state collapses to 0",       // W_SSM
    "MoE active_experts out of range (0 or > total_experts)",       // W_MOE
];

// ---------------------------------------------------------------------------
// KV cache formula dispatch
// ---------------------------------------------------------------------------

/// Compute the KV cache bytes for a given `ModelSpec`.
///
/// All byte counts are in `u64`. Overflow saturates to `u64::MAX`.
/// The formula per `AttentionKind` is in the rustdoc for
/// [`AttentionKind`](crate::attention::AttentionKind).
#[must_use]
pub fn kv_cache_bytes(model: &ModelSpec) -> u64 {
    let bytes_per = dtype_bytes(model.kv_quant) as u64;
    let ctx = model.ctx_len as u64;
    let batch = 1u64; // Slice 2 is single-batch inference; multi-batch is a future slice.

    match model.attention {
        AttentionKind::MHA | AttentionKind::GQA => {
            // 2 (K and V) * n_layers * n_kv_heads * head_dim * bytes/elem * ctx * batch
            let per_token = 2u64
                .saturating_mul(model.n_layers as u64)
                .saturating_mul(model.n_kv_heads as u64)
                .saturating_mul(model.head_dim as u64)
                .saturating_mul(bytes_per);
            per_token.saturating_mul(ctx).saturating_mul(batch)
        }
        AttentionKind::MQA => {
            // Single K/V head shared across all Q heads.
            let per_token = 2u64
                .saturating_mul(model.n_layers as u64)
                .saturating_mul(model.head_dim as u64)
                .saturating_mul(bytes_per);
            per_token.saturating_mul(ctx).saturating_mul(batch)
        }
        AttentionKind::MLA => {
            // Compressed latent; kv_latent_dim << head_dim.
            let per_token = 2u64
                .saturating_mul(model.n_layers as u64)
                .saturating_mul(model.kv_latent_dim as u64)
                .saturating_mul(bytes_per);
            per_token.saturating_mul(ctx).saturating_mul(batch)
        }
        AttentionKind::SLIDING => {
            // Bounded by window_size, not ctx_len.
            let per_token = 2u64
                .saturating_mul(model.n_layers as u64)
                .saturating_mul(model.n_kv_heads as u64)
                .saturating_mul(model.head_dim as u64)
                .saturating_mul(bytes_per);
            per_token
                .saturating_mul(model.window_size as u64)
                .saturating_mul(batch)
        }
        AttentionKind::SSM => {
            // No KV cache; constant state per layer.
            (model.n_layers as u64)
                .saturating_mul(model.state_dim as u64)
                .saturating_mul(bytes_per)
        }
        AttentionKind::HYBRID => {
            // Sum of attention-block KV + SSM-block state.
            let attn_layers = if model.n_attn_layers == 0 {
                model.n_layers as u64
            } else {
                model.n_attn_layers as u64
            };
            let ssm_layers = (model.n_layers as u64).saturating_sub(attn_layers);
            let kv_per_token = 2u64
                .saturating_mul(attn_layers)
                .saturating_mul(model.n_kv_heads as u64)
                .saturating_mul(model.head_dim as u64)
                .saturating_mul(bytes_per);
            let kv = kv_per_token.saturating_mul(ctx).saturating_mul(batch);
            let state = ssm_layers
                .saturating_mul(model.state_dim as u64)
                .saturating_mul(bytes_per);
            kv.saturating_add(state)
        }
        AttentionKind::SINK => {
            // Attention sinks (sink_tokens) + sliding window (window_size).
            let bound = (model.sink_tokens as u64).saturating_add(model.window_size as u64);
            let per_token = 2u64
                .saturating_mul(model.n_layers as u64)
                .saturating_mul(model.n_kv_heads as u64)
                .saturating_mul(model.head_dim as u64)
                .saturating_mul(bytes_per);
            per_token.saturating_mul(bound).saturating_mul(batch)
        }
    }
}

// ---------------------------------------------------------------------------
// Full estimate
// ---------------------------------------------------------------------------

/// Compute the full VRAM estimate: weights + KV + activations + overhead.
///
/// The breakdown is per-axis for explainability (per
/// `NFR-HWL-EXPLAINABILITY-001`). The `warnings` field flags
/// under-specified inputs (e.g., MoE with `active_experts == 0`).
#[must_use]
pub fn estimate_vram(model: &ModelSpec) -> CapacityEstimate {
    // Weights (MoE-aware).
    let active_params = match model.moe {
        Some(moe) => moe.active_params(),
        None => model.params,
    };
    let weights = vram_estimate(active_params, model.quant);

    // KV cache (architecture-keyed).
    let kv_cache = kv_cache_bytes(model);

    // Activations: 2 (forward + output grad scratch) * batch * ctx * hidden * 2 (fp16).
    // Conservative; the typical transformer activation peak is around
    // `2 * batch * seq * hidden` in fp16. We use 2× as a safety
    // margin for K/V gather + softmax + attention output.
    let activations = 2u64
        .saturating_mul(model.ctx_len as u64)
        .saturating_mul(model.hidden as u64)
        .saturating_mul(2);

    // Overhead: max(weights * 0.05, 256 MiB).
    let five_pct = weights / 20;
    let reserve: u64 = 256 * 1024 * 1024;
    let overhead = five_pct.max(reserve);

    // Sum (saturated).
    let total = weights
        .saturating_add(kv_cache)
        .saturating_add(activations)
        .saturating_add(overhead);

    // Warnings (under-specified inputs) — collected as a bitfield so
    // `CapacityEstimate` stays `Copy` and `no_std`-friendly.
    let mut warning_flags: WarningFlags = 0;
    if model.attention == AttentionKind::MLA && model.kv_latent_dim == 0 {
        warning_flags |= W_MLA;
    }
    if model.attention == AttentionKind::SLIDING && model.window_size == 0 {
        warning_flags |= W_SLIDING;
    }
    if model.attention == AttentionKind::SINK
        && (model.sink_tokens == 0 || model.window_size == 0)
    {
        warning_flags |= W_SINK;
    }
    if model.attention == AttentionKind::SSM && model.state_dim == 0 {
        warning_flags |= W_SSM;
    }
    if let Some(moe) = model.moe {
        if moe.active_experts == 0 || moe.active_experts > moe.total_experts {
            warning_flags |= W_MOE;
        }
    }

    CapacityEstimate {
        weights,
        kv_cache,
        activations,
        overhead,
        total,
        warning_flags,
    }
}

/// Return the set of warning labels for the given flags, in canonical
/// (bit-position) order. The result is a small `alloc::vec::Vec` (at
/// most 5 entries); callers that need a `no_std`-friendly check should
/// use `estimate.has_warning(flag)` directly.
#[cfg(feature = "alloc")]
pub fn warning_labels(flags: WarningFlags) -> alloc::vec::Vec<&'static str> {
    let mut out: alloc::vec::Vec<&'static str> = alloc::vec::Vec::new();
    for (i, label) in WARNING_LABELS.iter().enumerate() {
        if (flags & (1 << i)) != 0 {
            out.push(*label);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Fit score / verdict
// ---------------------------------------------------------------------------

/// Headroom ratio in `[0.0, 1.0]`. Higher = more free VRAM after the
/// model loads. `0.0` means the model exactly fills the device (will
/// OOM on any KV growth); `1.0` means the model uses 0 % of the
/// device (impossible in practice).
///
/// Formula: `clamp((device.vram - estimate.total) / device.vram, 0.0, 1.0)`.
/// `NaN` is returned only if `device.vram == 0` (degenerate input);
/// callers should treat that as `Fail`.
#[must_use]
pub fn fit_score(estimate: &CapacityEstimate, device: &DeviceSpec) -> f32 {
    if device.vram_bytes == 0 {
        return 0.0;
    }
    if estimate.total >= device.vram_bytes {
        return 0.0;
    }
    let free = (device.vram_bytes - estimate.total) as f64;
    let ratio = free / (device.vram_bytes as f64);
    // Clamp + cast to f32.
    ratio.clamp(0.0, 1.0) as f32
}

/// Three-way fit judgment. Thresholds in `policy::THRESHOLD_TIGHT` and
/// `policy::THRESHOLD_FAIL`. Both `fit_score` and `fit_verdict`
/// consume the same constants — no drift.
#[must_use]
pub fn fit_verdict(estimate: &CapacityEstimate, device: &DeviceSpec) -> FitVerdict {
    let score = fit_score(estimate, device);
    if score >= THRESHOLD_TIGHT {
        FitVerdict::Fit
    } else if score >= THRESHOLD_FAIL {
        FitVerdict::Tight
    } else {
        FitVerdict::Fail
    }
}

// ---------------------------------------------------------------------------
// Convenience public API (L5-115) — the "from-a-spec" wrapper layer.
// ---------------------------------------------------------------------------

/// Convenience: total VRAM bytes for a `ModelSpec`.
///
/// Equivalent to `estimate_vram(model).total`. The user's preferred
/// public-API shape is `fn estimate_vram(model) -> u64`; this is the
/// version that returns just the total. The breakdown form is
/// `estimate_vram(model) -> CapacityEstimate` (also public).
#[must_use]
pub fn estimate_total_vram(model: &ModelSpec) -> u64 {
    estimate_vram(model).total
}

/// Fit-score bundle: verdict + headroom bytes + ratio. Returned by
/// `assess_fit` for the user's preferred public-API shape
/// (`fn fit_score(model, target_vram) -> f32` returns just the ratio;
/// this struct bundles all three for callers that want more context).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitScore {
    /// Three-way judgment. See [`FitVerdict`].
    pub verdict: FitVerdict,
    /// Free bytes after the model loads. `0` if the model does not fit.
    pub headroom_bytes: u64,
    /// Headroom ratio in `[0.0, 1.0]`. See [`fit_score`].
    pub fit_score: f32,
}

/// Convenience: assess fit for a `ModelSpec` against a target device
/// in raw bytes. Returns a [`FitScore`] bundling verdict, headroom,
/// and the ratio.
///
/// This is the user's preferred public-API entry point
/// (`fn fit_score(model, target_vram) -> f32`); the structured return
/// here is a superset (callers that want only the ratio can use
/// `.fit_score`).
#[must_use]
pub fn assess_fit(model: &ModelSpec, target_vram_bytes: u64) -> FitScore {
    let est = estimate_vram(model);
    let device = DeviceSpec {
        vram_bytes: target_vram_bytes,
        name: "",
    };
    let headroom = target_vram_bytes.saturating_sub(est.total);
    FitScore {
        verdict: fit_verdict(&est, &device),
        headroom_bytes: headroom,
        fit_score: fit_score(&est, &device),
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // LLaMA-3-8B fixture (GQA, FP16, 8B params, 32 layers, 8 KV heads,
    // 128 head_dim, 4096 hidden, 8K ctx).
    fn llama3_8b() -> ModelSpec {
        ModelSpec {
            params: 8_000_000_000,
            attention: AttentionKind::GQA,
            ctx_len: 8192,
            n_layers: 32,
            n_kv_heads: 8,
            head_dim: 128,
            hidden: 4096,
            moe: None,
            quant: Dtype::F16,
            kv_quant: Dtype::F16,
            ..Default::default()
        }
    }

    // DeepSeek-V2-Lite (MLA, FP16, 16B params, 27 layers, kv_latent_dim=512).
    fn deepseek_v2_lite() -> ModelSpec {
        ModelSpec {
            params: 16_000_000_000,
            attention: AttentionKind::MLA,
            ctx_len: 32_000,
            n_layers: 27,
            n_kv_heads: 0,
            head_dim: 128,
            hidden: 4096,
            moe: None,
            quant: Dtype::F16,
            kv_quant: Dtype::F16,
            kv_latent_dim: 512,
            ..Default::default()
        }
    }

    // Mamba-2-2.7B (SSM, no KV, FP16, 2.7B params, 64 layers, state_dim=128).
    fn mamba2_2_7b() -> ModelSpec {
        ModelSpec {
            params: 2_700_000_000,
            attention: AttentionKind::SSM,
            ctx_len: 0, // SSM doesn't use ctx_len
            n_layers: 64,
            n_kv_heads: 0,
            head_dim: 0,
            hidden: 2560,
            moe: None,
            quant: Dtype::F16,
            kv_quant: Dtype::F16,
            state_dim: 128,
            ..Default::default()
        }
    }

    // Mixtral-8x7B (GQA + MoE, 14B shared + 8 experts × 4.25B, 2 active).
    fn mixtral_8x7b() -> ModelSpec {
        ModelSpec {
            params: 14_000_000_000, // shared_params (also used for attention + embed)
            attention: AttentionKind::GQA,
            ctx_len: 32_000,
            n_layers: 32,
            n_kv_heads: 8,
            head_dim: 128,
            hidden: 4096,
            moe: Some(MoEConfig {
                total_experts: 8,
                active_experts: 2,
                expert_params: 4_250_000_000,
                shared_params: 14_000_000_000,
            }),
            quant: Dtype::F16,
            kv_quant: Dtype::F16,
            ..Default::default()
        }
    }

    #[test]
    fn moe_active_params_is_shared_plus_active_experts() {
        let moe = MoEConfig {
            total_experts: 8,
            active_experts: 2,
            expert_params: 4_250_000_000,
            shared_params: 14_000_000_000,
        };
        // active = 14B + 2*4.25B = 22.5B
        assert_eq!(moe.active_params(), 14_000_000_000 + 2 * 4_250_000_000);
    }

    #[test]
    fn moe_active_params_saturates_on_overflow() {
        let moe = MoEConfig {
            total_experts: 8,
            active_experts: 2,
            expert_params: u64::MAX,
            shared_params: 1,
        };
        // Saturates to u64::MAX rather than panicking.
        assert_eq!(moe.active_params(), u64::MAX);
    }

    #[test]
    fn kv_gqa_is_linear_in_ctx_and_layers() {
        let model = llama3_8b();
        let kv_8k = kv_cache_bytes(&model);
        // 2 * 32 * 8 * 128 * 2 (F16) * 8192 = 1_073_741_824 (1 GiB).
        assert_eq!(kv_8k, 1_073_741_824);
    }

    #[test]
    fn kv_mla_uses_latent_dim_not_head_dim() {
        let model = deepseek_v2_lite();
        let kv = kv_cache_bytes(&model);
        // 2 * 27 * 512 (latent) * 2 (F16) * 32000 = 1_769_472_000.
        // This is MUCH smaller than GQA at the same hidden/head_dim.
        assert!(kv < 2_000_000_000);
    }

    #[test]
    fn kv_ssm_is_constant_in_ctx() {
        let mut model = mamba2_2_7b();
        // Try various ctx_len values; KV (state) should be the same.
        model.ctx_len = 0;
        let s_0 = kv_cache_bytes(&model);
        model.ctx_len = 1_000_000;
        let s_huge = kv_cache_bytes(&model);
        // 64 * 128 * 2 = 16384 bytes.
        assert_eq!(s_0, s_huge);
        assert_eq!(s_0, 64 * 128 * 2);
    }

    #[test]
    fn kv_sliding_is_bounded_by_window() {
        let mut model = llama3_8b();
        model.attention = AttentionKind::SLIDING;
        model.window_size = 4096;
        let kv_win = kv_cache_bytes(&model);
        // 2 * 32 * 8 * 128 * 2 (F16) * 4096 = 536_870_912 (512 MiB).
        assert_eq!(kv_win, 536_870_912);
        // Doubling ctx_len should NOT change the KV cache.
        model.ctx_len = 1_000_000;
        let kv_huge = kv_cache_bytes(&model);
        assert_eq!(kv_huge, kv_win);
    }

    #[test]
    fn kv_sink_uses_sink_plus_window() {
        let mut model = llama3_8b();
        model.attention = AttentionKind::SINK;
        model.sink_tokens = 4;
        model.window_size = 4096;
        let kv = kv_cache_bytes(&model);
        // 2 * 32 * 8 * 128 * 2 (F16) * (4 + 4096) = 537_395_200.
        assert_eq!(kv, 537_395_200);
    }

    #[test]
    fn estimate_llama3_8b_on_a100_40gb_fits() {
        let model = llama3_8b();
        let est = estimate_vram(&model);
        // Weights: 8B * 2 = 16 GB.
        assert_eq!(est.weights, 16_000_000_000);
        // Total < 40 GB → fits.
        let a100: DeviceSpec = DeviceSpec {
            vram_bytes: 40 * 1024_u64.pow(3),
            name: "A100-40GB",
        };
        let verdict = fit_verdict(&est, &a100);
        assert!(verdict == FitVerdict::Fit || verdict == FitVerdict::Tight);
    }

    #[test]
    fn estimate_mixtral_uses_moe_active_params() {
        let model = mixtral_8x7b();
        let est = estimate_vram(&model);
        // Active params = 14B + 2*4.25B = 22.5B → 45 GB weights (FP16).
        assert_eq!(est.weights, 45_000_000_000);
    }

    #[test]
    fn fit_score_zero_when_model_does_not_fit() {
        // 8B FP16 = 16 GB; on a 8 GB device, no fit.
        let model = llama3_8b();
        let est = estimate_vram(&model);
        let dev = DeviceSpec {
            vram_bytes: 8 * 1024_u64.pow(3),
            name: "RTX-3060-8GB",
        };
        assert_eq!(fit_score(&est, &dev), 0.0);
        assert_eq!(fit_verdict(&est, &dev), FitVerdict::Fail);
    }

    #[test]
    fn fit_score_one_when_model_is_tiny() {
        // 1M-param FP16 = 2 MB; on a 40 GB device, very comfortable.
        let model = ModelSpec {
            params: 1_000_000,
            attention: AttentionKind::GQA,
            ctx_len: 1024,
            n_layers: 4,
            n_kv_heads: 4,
            head_dim: 32,
            hidden: 256,
            quant: Dtype::F16,
            kv_quant: Dtype::F16,
            ..Default::default()
        };
        let est = estimate_vram(&model);
        let dev = DeviceSpec {
            vram_bytes: 40 * 1024_u64.pow(3),
            name: "A100-40GB",
        };
        let score = fit_score(&est, &dev);
        // Tiny model, lots of free VRAM → score is very close to 1.0.
        assert!(score > 0.99);
        assert_eq!(fit_verdict(&est, &dev), FitVerdict::Fit);
    }

    #[test]
    fn fit_score_zero_when_device_vram_is_zero() {
        let model = llama3_8b();
        let est = estimate_vram(&model);
        let dev = DeviceSpec {
            vram_bytes: 0,
            name: "degenerate",
        };
        // Degenerate input: degenerate score, Fail verdict.
        assert_eq!(fit_score(&est, &dev), 0.0);
        assert_eq!(fit_verdict(&est, &dev), FitVerdict::Fail);
    }

    #[test]
    fn warnings_flag_under_specified_mla() {
        let mut model = deepseek_v2_lite();
        model.kv_latent_dim = 0; // Oops, MLA without latent dim.
        let est = estimate_vram(&model);
        assert!(est.has_warning(W_MLA));
    }

    #[test]
    fn warnings_flag_under_specified_ssm() {
        let mut model = mamba2_2_7b();
        model.state_dim = 0;
        let est = estimate_vram(&model);
        assert!(est.has_warning(W_SSM));
    }

    #[test]
    fn warnings_flag_moe_active_out_of_range() {
        let mut model = mixtral_8x7b();
        model.moe = Some(MoEConfig {
            total_experts: 8,
            active_experts: 0,
            expert_params: 4_250_000_000,
            shared_params: 14_000_000_000,
        });
        let est = estimate_vram(&model);
        assert!(est.has_warning(W_MOE));
    }

    #[test]
    fn no_warnings_for_well_formed_spec() {
        let model = llama3_8b();
        let est = estimate_vram(&model);
        assert!(!est.has_any_warning());
        assert_eq!(est.warning_flags, 0);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn warning_labels_decode_all_five_bits() {
        // Set all 5 bits and decode; expect 5 labels in canonical order.
        extern crate alloc;
        use alloc::vec::Vec;
        let all: WarningFlags = W_MLA | W_SLIDING | W_SINK | W_SSM | W_MOE;
        let labels: Vec<&'static str> = warning_labels(all);
        assert_eq!(labels.len(), 5);
        assert_eq!(labels[0], WARNING_LABELS[0]);
        assert_eq!(labels[4], WARNING_LABELS[4]);
    }
}

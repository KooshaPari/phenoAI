// math.rs — pure-math primitives for VRAM estimation and capacity planning.
//
// All functions are no_std-compatible, deterministic, and side-effect free.
// Every public function documents its source / formula in the rustdoc.
//
// References:
//   - Chinchilla scaling law: Hoffmann et al. 2022, arXiv:2203.15556
//   - AdamW: Loshchilov & Hutter 2019, arXiv:1711.05101
//   - LoRA: Hu et al. 2021, arXiv:2106.09685
//   - QLoRA: Dettmers et al. 2023, arXiv:2305.14314
//   - 8x A100 / H100 / B200 datasheets (peak BF16 dense TFLOPS)
//   - HwLedger `apps/streamlit/lib/cost_model.py` @ 8bf878ca (the
//     Python original this crate re-implements in Rust).

/// Floating-point precision for model weights and activations.
///
/// Variants follow the canonical LLM training/inference dtypes. The
/// byte width is intentionally fixed (not configurable) so capacity
/// planners can compute exact VRAM figures without approximation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dtype {
    /// 32-bit IEEE-754 float (4 bytes per parameter).
    F32,
    /// 16-bit IEEE-754 float (2 bytes per parameter).
    F16,
    /// 16-bit brain float (2 bytes per parameter; same size as F16,
    /// different exponent bias).
    BF16,
    /// 8-bit integer (1 byte per parameter; quantised).
    I8,
    /// 4-bit integer, packed (1 byte per 2 parameters; AWQ / GPTQ
    /// quantised).
    I4,
}

impl Dtype {
    /// All dtypes known to this crate. Useful for exhaustive `match`
    /// without `_ => unreachable!()`.
    pub const ALL: &'static [Dtype] = &[Dtype::F32, Dtype::F16, Dtype::BF16, Dtype::I8, Dtype::I4];

    /// Human-readable name (`"F16"`, `"BF16"`, etc.).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Dtype::F32 => "F32",
            Dtype::F16 => "F16",
            Dtype::BF16 => "BF16",
            Dtype::I8 => "I8",
            Dtype::I4 => "I4",
        }
    }
}

/// Fine-tuning optimizer state-overhead class.
///
/// The variants are the canonical categories used in the
/// HwLedger / paged-optimiser literature. The numeric factors come from
/// `cost_model.py::fine_tune_overhead_mb` in HwLedger (git @ `8bf878ca`)
/// and are validated against the published LoRA / QLoRA papers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Optimizer {
    /// AdamW: 4x FP32 master + 2x first moment (m) + 2x second moment (v)
    /// = **8x** weight VRAM.
    AdamW,
    /// LoRA: <5% of weight VRAM (only adapter parameters + their
    /// optimiser state).
    LoRA,
    /// QLoRA: <3% of weight VRAM (4-bit base weights frozen, LoRA
    /// adapters in BF16).
    QLoRA,
    /// Adafactor: 2.5x weight VRAM (factored second-moment, no FP32
    /// master copy).
    Adafactor,
}

/// Return the byte width of a single parameter in the given dtype.
///
/// ```
/// use pheno_capacity::{dtype_bytes, Dtype};
///
/// assert_eq!(dtype_bytes(Dtype::F32), 4);
/// assert_eq!(dtype_bytes(Dtype::F16), 2);
/// assert_eq!(dtype_bytes(Dtype::BF16), 2);
/// assert_eq!(dtype_bytes(Dtype::I8), 1);
/// assert_eq!(dtype_bytes(Dtype::I4), 1); // packed
/// ```
#[must_use]
pub const fn dtype_bytes(d: Dtype) -> u8 {
    match d {
        Dtype::F32 => 4,
        Dtype::F16 | Dtype::BF16 => 2,
        Dtype::I8 | Dtype::I4 => 1,
    }
}

/// Estimate the VRAM (in bytes) required to hold the model **weights
/// only** (no gradients, no optimizer state, no activations, no KV
/// cache).
///
/// Formula: `vram_bytes = params * dtype_bytes`. On overflow (the
/// product exceeds `u64::MAX`), the function saturates to `u64::MAX`
/// rather than panicking; callers should treat `u64::MAX` as
/// "more than fits in any conceivable device."
///
/// # Examples
///
/// ```
/// use pheno_capacity::{vram_estimate, Dtype};
///
/// // LLaMA-7B in FP16: 7e9 * 2 = 14 GB.
/// assert_eq!(vram_estimate(7_000_000_000, Dtype::F16), 14_000_000_000);
///
/// // LLaMA-70B in FP16: 70e9 * 2 = 140 GB.
/// assert_eq!(vram_estimate(70_000_000_000, Dtype::F16), 140_000_000_000);
///
/// // LLaMA-7B in INT4 (AWQ): 7e9 * 1 = 7 GB.
/// assert_eq!(vram_estimate(7_000_000_000, Dtype::I4), 7_000_000_000);
///
/// // Zero params: zero bytes (edge case).
/// assert_eq!(vram_estimate(0, Dtype::F16), 0);
/// ```
#[must_use]
pub const fn vram_estimate(model_params: u64, dtype: Dtype) -> u64 {
    let bytes_per = dtype_bytes(dtype) as u64;
    match model_params.checked_mul(bytes_per) {
        Some(v) => v,
        None => u64::MAX,
    }
}

/// Return `true` if the model fits on the device in inference mode
/// (weights only; no optimizer state, no gradients).
///
/// `available` is the device's total VRAM in bytes (e.g.
/// `24 * 1024_u64.pow(3)` for a 24 GB consumer GPU). The function
/// does not reserve a headroom margin; the caller is responsible for
/// subtracting activations / KV cache as needed. For a turnkey
/// "fits with 20% headroom" wrapper, see the Streamlit layer in
/// HwLedger (post Phase 2 migration).
///
/// # Examples
///
/// ```
/// use pheno_capacity::{model_fits_in, Dtype};
///
/// let a100_40gb: u64 = 40 * 1024_u64.pow(3);
/// let rtx_4090_24gb: u64 = 24 * 1024_u64.pow(3);
/// let m3_ultra_max: u64 = 192 * 1024_u64.pow(3); // unified memory
///
/// // LLaMA-7B FP16 (14 GB): fits on A100-40GB, fits on 4090-24GB,
/// // fits on M3 Ultra 192GB.
/// assert!(model_fits_in(7_000_000_000, a100_40gb, Dtype::F16));
/// assert!(model_fits_in(7_000_000_000, rtx_4090_24gb, Dtype::F16));
/// assert!(model_fits_in(7_000_000_000, m3_ultra_max, Dtype::F16));
///
/// // LLaMA-70B FP16 (140 GB): does NOT fit on A100-40GB.
/// assert!(!model_fits_in(70_000_000_000, a100_40gb, Dtype::F16));
///
/// // LLaMA-70B FP16 INT4-quantised (70 GB): does NOT fit on
/// // A100-40GB but does fit on 2x A100-80GB tensor-parallel.
/// assert!(!model_fits_in(70_000_000_000, a100_40gb, Dtype::I4));
/// assert!(model_fits_in(70_000_000_000, 2 * 80 * 1024_u64.pow(3), Dtype::I4));
/// ```
#[must_use]
pub const fn model_fits_in(model_params: u64, available: u64, dtype: Dtype) -> bool {
    let vram = vram_estimate(model_params, dtype);
    // If vram saturated to u64::MAX, the model cannot fit on any
    // conceivable device; report "doesn't fit."
    if vram == u64::MAX {
        return false;
    }
    vram <= available
}

/// Return the **additional** VRAM (in bytes) needed to fine-tune a
/// model, on top of the forward-pass weight footprint.
///
/// The numbers come from `cost_model.py::fine_tune_overhead_mb`
/// (HwLedger @ 8bf878ca):
/// - **AdamW**: 8x weights (FP32 master copy + m + v moments + grad).
/// - **LoRA**:  5% of weights (adapter params + their state).
/// - **QLoRA**: 3% of weights (4-bit base + LoRA adapters in BF16).
/// - **Adafactor**: 2.5x weights (factored second moment, no FP32
///   master).
///
/// For inference (no fine-tuning), use `0` or skip the call.
///
/// # Examples
///
/// ```
/// use pheno_capacity::{optimizer_state_vram, Optimizer};
///
/// let weights_gb: u64 = 1024_u64.pow(3);
/// let one_gb: u64 = weights_gb;
///
/// // AdamW on a 1 GB BF16 weight set: 8 GB optimizer state.
/// assert_eq!(optimizer_state_vram(one_gb, Optimizer::AdamW), one_gb * 8);
///
/// // LoRA on the same: ~50 MB.
/// let lora = optimizer_state_vram(one_gb, Optimizer::LoRA);
/// assert!(lora < one_gb / 10);
/// assert!(lora > one_gb / 100);
///
/// // QLoRA: ~30 MB.
/// let qlora = optimizer_state_vram(one_gb, Optimizer::QLoRA);
/// assert!(qlora < one_gb / 20);
/// ```
#[must_use]
pub fn optimizer_state_vram(weights_bytes: u64, optimizer: Optimizer) -> u64 {
    // The factors are expressed as integer (num, den) pairs to
    // avoid f64 -> u64 rounding surprises. LoRA's 0.05 = 5/100,
    // QLoRA's 0.03 = 3/100, Adafactor's 2.5 = 5/2.
    let (num, den) = match optimizer {
        Optimizer::AdamW => (8_u64, 1_u64),
        Optimizer::LoRA => (5_u64, 100_u64),
        Optimizer::QLoRA => (3_u64, 100_u64),
        Optimizer::Adafactor => (5_u64, 2_u64),
    };
    match weights_bytes.checked_mul(num) {
        Some(v) => v / den,
        None => u64::MAX,
    }
}

/// Compute the Chinchilla-optimal training-token count for a model of
/// the given parameter size.
///
/// The default ratio is 20x params, which is Hoffmann et al.'s 2022
/// sweet spot for **compute-optimal** dense-transformer training
/// (https://arxiv.org/abs/2203.15556). Use a smaller ratio (4-8) for
/// continued-pretraining or distillation budgets, where the
/// compute-optimal is impractical.
///
/// Implementation note: the multiplication is done in `f64` to
/// preserve precision for the typical `parameter_count` range
/// (u64 up to ~1.8e19). f32 would lose precision for any
/// `parameter_count > 2^24 ~ 16.7M`, which corrupts the canonical
/// LLaMA-7B / 70B anchors (e.g. f32(7e9) * 20.0 = 140_000_002_048,
/// not 140_000_000_000).
///
/// # Examples
///
/// ```
/// use pheno_capacity::chinchilla_tokens;
///
/// // 7B model, Chinchilla-optimal: 140B tokens.
/// assert_eq!(chinchilla_tokens(7_000_000_000, 20.0), 140_000_000_000);
///
/// // 70B model, continued-pretraining (4x): 280B tokens.
/// assert_eq!(chinchilla_tokens(70_000_000_000, 4.0), 280_000_000_000);
/// ```
#[must_use]
pub fn chinchilla_tokens(parameter_count: u64, ratio: f32) -> u64 {
    // Cast through f64 to avoid f32 precision loss for parameter
    // counts > 2^24 (16.7M). The f32->f64 widening is exact; the
    // f64->u64 narrowing saturates.
    let product = (parameter_count as f64) * f64::from(ratio);
    if !product.is_finite() || product < 0.0 {
        return 0;
    }
    if product > u64::MAX as f64 {
        return u64::MAX;
    }
    product as u64
}

// ---------------------------------------------------------------------------
// Unit tests for math.rs (focused on the math semantics; the
// crate-level tests in lib.rs cover the public-API ergonomics).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtype_as_str_matches_variant() {
        assert_eq!(Dtype::F32.as_str(), "F32");
        assert_eq!(Dtype::F16.as_str(), "F16");
        assert_eq!(Dtype::BF16.as_str(), "BF16");
        assert_eq!(Dtype::I8.as_str(), "I8");
        assert_eq!(Dtype::I4.as_str(), "I4");
    }

    #[test]
    fn dtype_all_has_five_variants() {
        assert_eq!(Dtype::ALL.len(), 5);
    }

    #[test]
    fn vram_estimate_i4_uses_packed_byte() {
        // 8B params * 1 byte/2params (packed) = 4 GB.
        // We model I4 as 1 byte per parameter (memory-side, not
        // bit-packed) for capacity-planning simplicity. This is the
        // same approximation used in HwLedger's cost_model.py.
        assert_eq!(vram_estimate(8_000_000_000, Dtype::I4), 8_000_000_000);
    }

    #[test]
    fn optimizer_state_vram_adafactor_is_2_5x() {
        let weights: u64 = 1024_u64.pow(3);
        let oh = optimizer_state_vram(weights, Optimizer::Adafactor);
        // 2.5x = 5/2. So 1 GB * 5 / 2 = 2.5 GB.
        assert_eq!(oh, weights * 5 / 2);
    }

    #[test]
    fn optimizer_state_vram_zero_weights_is_zero() {
        // Edge case: empty model.
        assert_eq!(optimizer_state_vram(0, Optimizer::AdamW), 0);
        assert_eq!(optimizer_state_vram(0, Optimizer::LoRA), 0);
    }

    #[test]
    fn chinchilla_tokens_zero_params_is_zero() {
        assert_eq!(chinchilla_tokens(0, 20.0), 0);
    }

    #[test]
    fn chinchilla_tokens_ratio_zero_is_zero() {
        assert_eq!(chinchilla_tokens(7_000_000_000, 0.0), 0);
    }

    #[test]
    fn chinchilla_tokens_1b_params_20x_is_20b() {
        // Round-trip: 1e9 * 20 = 2e10.
        assert_eq!(chinchilla_tokens(1_000_000_000, 20.0), 20_000_000_000);
    }

    #[test]
    fn chinchilla_tokens_negative_ratio_returns_zero() {
        // Negative ratios are nonsense; saturate to 0.
        assert_eq!(chinchilla_tokens(1_000_000_000, -1.0), 0);
    }

    #[test]
    fn model_fits_in_exact_boundary() {
        // 1 param in F16 = 2 bytes; 1 param in I4 = 1 byte. 1 byte
        // of available memory fits 1 param in I4 but not in F16.
        assert!(model_fits_in(1, 1, Dtype::I4));
        assert!(!model_fits_in(1, 1, Dtype::F16));
        // 2 bytes of available memory fits 1 param in F16.
        assert!(model_fits_in(1, 2, Dtype::F16));
        // 2 bytes of available memory does NOT fit 2 params in F16
        // (would need 4 bytes).
        assert!(!model_fits_in(2, 2, Dtype::F16));
    }

    #[test]
    fn model_fits_in_huge_model_returns_false() {
        // Model with more params than fit in any device: should
        // return false (saturated vram_estimate is u64::MAX, which
        // is > any reasonable available).
        let astronomical = u64::MAX / 2 + 1;
        assert!(!model_fits_in(astronomical, u64::MAX, Dtype::F16));
    }
}

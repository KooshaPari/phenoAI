// attention.rs — per-attention-kind KV cache formula dispatch.
//
// The KV cache is the dominant memory cost for long-context LLM inference.
// The formula varies substantially by attention architecture:
//   - MHA / MQA / GQA: classic 2 * n_layers * n_kv_heads * head_dim * seq * batch
//   - MLA (DeepSeek-V2): compressed via low-rank latent; kv_latent_dim << head_dim
//   - SLIDING (Mistral): constant in seq_len; bounded by window_size
//   - SSM (Mamba): no KV cache at all; constant state instead
//   - HYBRID (Jamba): interleaved attention + SSM blocks; per-block sum
//   - SINK (StreamingLLM): attention sinks + sliding window
//
// References (see docs/methodology.md for full citation list):
//   - MHA / MQA / GQA: Vaswani 2017 (arXiv:1706.03762), Shazeer 2019 (arXiv:1911.02150), Ainslie 2023 (arXiv:2305.13245)
//   - MLA: DeepSeek-V2 (arXiv:2405.04347)
//   - Sliding window: Mistral 7B (arXiv:2310.06825)
//   - SSM: Mamba (arXiv:2312.00752), Mamba-2 (arXiv:2405.21060)
//   - Hybrid: Jamba (arXiv:2403.19887)
//   - Sink: StreamingLLM (arXiv:2309.17453)

use crate::math::{dtype_bytes, Dtype};

/// Architecture-keyed attention kind. Dispatches the KV cache formula.
///
/// The variants are the canonical categories from the post-2023 LLM
/// literature. Each one has a distinct KV cache formula — getting the
/// dispatch wrong is the main reason prior public VRAM calculators
/// (HF Accelerate, can-it-run-llm, LM Studio's gauge) under-count KV
/// for MoE/MLA and over-count for SSM/sliding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttentionKind {
    /// Multi-Head Attention (Transformer original).
    /// Every head has its own K/V projection. KV per token:
    /// `2 * n_layers * n_kv_heads * head_dim` (= 2 * n_layers * head_dim * n_heads).
    MHA,

    /// Multi-Query Attention (Shazeer 2019).
    /// All heads share a single K/V head. KV per token:
    /// `2 * n_layers * 1 * head_dim` — much smaller than MHA at the same
    /// hidden size.
    MQA,

    /// Grouped-Query Attention (Ainslie 2023).
    /// `n_kv_heads` K/V heads shared across `n_heads / n_kv_heads` Q
    /// heads. This is the canonical modern attention (LLaMA-2/3,
    /// Mistral, Qwen2). KV per token:
    /// `2 * n_layers * n_kv_heads * head_dim`.
    GQA,

    /// Multi-Latent Attention (DeepSeek-V2).
    /// Compresses K/V through a low-rank latent; `kv_latent_dim` is
    /// typically `head_dim / 4` or smaller. KV per token:
    /// `2 * n_layers * kv_latent_dim` — much smaller than GQA.
    /// Use this for DeepSeek-V2 / V3.
    MLA,

    /// Sliding-window attention (Mistral).
    /// KV cache is bounded by `window_size`, independent of `ctx_len`.
    /// KV per token (effective):
    /// `2 * n_layers * n_kv_heads * head_dim * window_size` (constant in ctx).
    /// Use this for Mistral 7B, Qwen2 with sliding window.
    SLIDING,

    /// State-Space Model (Mamba).
    /// No KV cache. The state is a small constant per layer
    /// (`state_dim * bytes`), independent of sequence length. Use this
    /// for Mamba / Mamba-2 / Jamba SSM blocks.
    SSM,

    /// Hybrid (Jamba-style): interleaved attention + SSM blocks.
    /// The KV cache is the sum of per-attention-block contributions
    /// (SSM blocks contribute 0 to KV). The formula is:
    /// `2 * n_attn_layers * n_kv_heads * head_dim * seq_len * batch`.
    HYBRID,

    /// Attention Sinks (StreamingLLM).
    /// Keeps `sink_tokens` initial tokens + a sliding window of
    /// `window_size` tokens per layer. KV per token (effective):
    /// `2 * n_layers * n_kv_heads * (sink_tokens + window_size)` (constant).
    SINK,
}

impl Default for AttentionKind {
    /// Canonical default: GQA (the dominant post-2023 attention
    /// pattern — LLaMA-2/3, Mistral, Qwen2).
    fn default() -> Self {
        AttentionKind::GQA
    }
}

impl AttentionKind {
    /// All variants known to this crate. Useful for exhaustive `match`
    /// without `_ => unreachable!()`.
    pub const ALL: &'static [AttentionKind] = &[
        AttentionKind::MHA,
        AttentionKind::MQA,
        AttentionKind::GQA,
        AttentionKind::MLA,
        AttentionKind::SLIDING,
        AttentionKind::SSM,
        AttentionKind::HYBRID,
        AttentionKind::SINK,
    ];

    /// Canonical human-readable name (`"MHA"`, `"MLA"`, etc.).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            AttentionKind::MHA => "MHA",
            AttentionKind::MQA => "MQA",
            AttentionKind::GQA => "GQA",
            AttentionKind::MLA => "MLA",
            AttentionKind::SLIDING => "SLIDING",
            AttentionKind::SSM => "SSM",
            AttentionKind::HYBRID => "HYBRID",
            AttentionKind::SINK => "SINK",
        }
    }

    /// `true` if this attention kind has a constant-in-ctx_len KV cache.
    /// SSM is the only zero-KV kind. SLIDING and SINK are bounded-KV.
    #[must_use]
    pub const fn is_constant_in_seq(self) -> bool {
        matches!(
            self,
            AttentionKind::SSM | AttentionKind::SLIDING | AttentionKind::SINK
        )
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attention_as_str_matches_variant() {
        for &k in AttentionKind::ALL {
            // All names are short uppercase strings.
            let s = k.as_str();
            assert!(s.chars().all(|c| c.is_ascii_uppercase()));
            assert!(s.len() <= 8);
        }
    }

    #[test]
    fn attention_all_has_eight_variants() {
        assert_eq!(AttentionKind::ALL.len(), 8);
    }

    #[test]
    fn attention_constant_in_seq_flags() {
        // SSM, SLIDING, SINK are constant in seq_len.
        assert!(AttentionKind::SSM.is_constant_in_seq());
        assert!(AttentionKind::SLIDING.is_constant_in_seq());
        assert!(AttentionKind::SINK.is_constant_in_seq());
        // MHA, MQA, GQA, MLA, HYBRID are linear in seq_len.
        assert!(!AttentionKind::MHA.is_constant_in_seq());
        assert!(!AttentionKind::MQA.is_constant_in_seq());
        assert!(!AttentionKind::GQA.is_constant_in_seq());
        assert!(!AttentionKind::MLA.is_constant_in_seq());
        assert!(!AttentionKind::HYBRID.is_constant_in_seq());
    }
}

// ---------------------------------------------------------------------------
// KvContext + estimate_kv_vram — slice 1 (L5-115) public surface
// ---------------------------------------------------------------------------

/// Reference constant: bytes per token per layer for DeepSeek-V2/V3 MLA
/// compressed latent. Roughly `2 * kv_latent_dim * bytes_per_elem` for
/// the canonical `kv_latent_dim = 512, dtype = F16` config
/// (= 2 * 512 * 2 = 2048 bytes/token/layer).
///
/// Used by `recommended_batch_size` to make MLA-vs-GQA comparisons
/// apples-to-apples.
pub const KV_LATENT_BYTES_PER_TOKEN_PER_LAYER_DEEPSEEK: u64 = 2048;

/// The KV-cache-relevant context for a model + a request.
///
/// `seq_len` here is the **max** sequence length the request will reach
/// (not the current length). For SINK/SLIDING patterns the effective
/// KV is bounded by `window_size + sink_tokens` regardless of `seq_len`,
/// but we still set `seq_len` to the planned length for downstream
/// linear-in-`seq_len` consumers (vLLM, TGI, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KvContext {
    /// Number of concurrent requests sharing the KV cache.
    pub batch_size: u32,
    /// Planned (max) sequence length per request, in tokens.
    pub seq_len: u32,
    /// Number of transformer blocks.
    pub num_layers: u32,
    /// Number of K/V heads. For MQA this should be 1; for MLA/SSM set to 0
    /// (the latent dim / state dim are encoded elsewhere).
    pub num_kv_heads: u32,
    /// Per-head dimension (e.g. 128 for LLaMA). Set to 0 for SSM/MLA.
    pub head_dim: u32,
    /// Attention architecture. Dispatches the KV formula.
    pub attention: AttentionKind,
    /// MLA only: low-rank latent dimension. Set to 0 for non-MLA.
    pub kv_latent_dim: u32,
    /// SLIDING / SINK only: window size in tokens. Set to 0 otherwise.
    pub window_size: u32,
    /// SINK only: number of attention-sink tokens. Set to 0 otherwise.
    pub sink_tokens: u32,
    /// SSM only: state-space state dimension. Set to 0 otherwise.
    pub state_dim: u32,
    /// HYBRID only: number of attention blocks (vs SSM blocks).
    /// If 0, defaults to `num_layers` (i.e., all blocks are attention).
    pub num_attn_layers: u32,
    /// KV cache dtype (typically F16 or BF16; KIVI uses I8).
    pub kv_quant: Dtype,
}

impl Default for KvContext {
    fn default() -> Self {
        Self {
            batch_size: 1,
            seq_len: 0,
            num_layers: 0,
            num_kv_heads: 0,
            head_dim: 0,
            attention: AttentionKind::GQA,
            kv_latent_dim: 0,
            window_size: 0,
            sink_tokens: 0,
            state_dim: 0,
            num_attn_layers: 0,
            kv_quant: Dtype::F16,
        }
    }
}

/// Compute the KV cache bytes for a `KvContext`.
///
/// All byte counts are `u64`. Overflow saturates to `u64::MAX`. The
/// formula per `AttentionKind` is documented on the enum.
///
/// **Note:** this is the **per-batch** KV. Multiply by `batch_size` if
/// you need total (already done internally; the `batch_size` field is
/// used for that).
#[must_use]
pub fn estimate_kv_vram(ctx: &KvContext, kv_quant: Dtype) -> u64 {
    let bytes_per = dtype_bytes(kv_quant) as u64;
    let seq = ctx.seq_len as u64;
    let batch = ctx.batch_size.max(1) as u64;
    let layers = ctx.num_layers as u64;

    match ctx.attention {
        AttentionKind::MHA | AttentionKind::GQA => {
            // 2 (K+V) * layers * n_kv_heads * head_dim * bytes/elem * seq * batch
            let per_token = 2u64
                .saturating_mul(layers)
                .saturating_mul(ctx.num_kv_heads as u64)
                .saturating_mul(ctx.head_dim as u64)
                .saturating_mul(bytes_per);
            per_token.saturating_mul(seq).saturating_mul(batch)
        }
        AttentionKind::MQA => {
            // Single shared K/V head.
            let per_token = 2u64
                .saturating_mul(layers)
                .saturating_mul(ctx.head_dim as u64)
                .saturating_mul(bytes_per);
            per_token.saturating_mul(seq).saturating_mul(batch)
        }
        AttentionKind::MLA => {
            // Compressed latent: 2 (K+V) * layers * kv_latent_dim * bytes * seq * batch
            let per_token = 2u64
                .saturating_mul(layers)
                .saturating_mul(ctx.kv_latent_dim as u64)
                .saturating_mul(bytes_per);
            per_token.saturating_mul(seq).saturating_mul(batch)
        }
        AttentionKind::SLIDING => {
            // Bounded by window_size.
            let per_token = 2u64
                .saturating_mul(layers)
                .saturating_mul(ctx.num_kv_heads as u64)
                .saturating_mul(ctx.head_dim as u64)
                .saturating_mul(bytes_per);
            per_token
                .saturating_mul(ctx.window_size as u64)
                .saturating_mul(batch)
        }
        AttentionKind::SSM => {
            // No KV; constant state per layer.
            layers
                .saturating_mul(ctx.state_dim as u64)
                .saturating_mul(bytes_per)
                .saturating_mul(batch)
        }
        AttentionKind::HYBRID => {
            // Attention-block KV + SSM-block state.
            let attn_layers = if ctx.num_attn_layers == 0 {
                layers
            } else {
                ctx.num_attn_layers as u64
            };
            let ssm_layers = layers.saturating_sub(attn_layers);
            let kv_per_token = 2u64
                .saturating_mul(attn_layers)
                .saturating_mul(ctx.num_kv_heads as u64)
                .saturating_mul(ctx.head_dim as u64)
                .saturating_mul(bytes_per);
            let kv = kv_per_token.saturating_mul(seq).saturating_mul(batch);
            let state = ssm_layers
                .saturating_mul(ctx.state_dim as u64)
                .saturating_mul(bytes_per)
                .saturating_mul(batch);
            kv.saturating_add(state)
        }
        AttentionKind::SINK => {
            // Attention sinks + sliding window.
            let bound = (ctx.sink_tokens as u64).saturating_add(ctx.window_size as u64);
            let per_token = 2u64
                .saturating_mul(layers)
                .saturating_mul(ctx.num_kv_heads as u64)
                .saturating_mul(ctx.head_dim as u64)
                .saturating_mul(bytes_per);
            per_token.saturating_mul(bound).saturating_mul(batch)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests for KvContext + estimate_kv_vram
// ---------------------------------------------------------------------------

#[cfg(test)]
mod kv_tests {
    use super::*;

    #[test]
    fn kv_gqa_8b_at_8k_ctx_is_one_gib() {
        // LLaMA-3-8B: 32 layers, 8 KV heads, 128 head_dim, F16.
        // At 8K ctx, batch 1: 2 * 32 * 8 * 128 * 2 * 8192 = 1_073_741_824 (1 GiB).
        let ctx = KvContext {
            batch_size: 1,
            seq_len: 8192,
            num_layers: 32,
            num_kv_heads: 8,
            head_dim: 128,
            attention: AttentionKind::GQA,
            kv_quant: Dtype::F16,
            ..Default::default()
        };
        assert_eq!(estimate_kv_vram(&ctx, Dtype::F16), 1_073_741_824);
    }

    #[test]
    fn kv_gqa_scales_linearly_with_batch() {
        // Doubling batch doubles KV.
        let mut ctx = KvContext {
            batch_size: 1,
            seq_len: 1024,
            num_layers: 4,
            num_kv_heads: 4,
            head_dim: 32,
            attention: AttentionKind::GQA,
            kv_quant: Dtype::F16,
            ..Default::default()
        };
        let b1 = estimate_kv_vram(&ctx, Dtype::F16);
        ctx.batch_size = 2;
        let b2 = estimate_kv_vram(&ctx, Dtype::F16);
        assert_eq!(b2, b1 * 2);
    }

    #[test]
    fn kv_mla_is_much_smaller_than_gqa() {
        // DeepSeek-V2-Lite: 27 layers, kv_latent_dim=512, F16, 32K ctx.
        // 2 * 27 * 512 * 2 * 32000 = 1_769_472_000 (~1.65 GiB).
        // Compare to a hypothetical GQA at head_dim=128: would be
        // 2 * 27 * 32 * 128 * 2 * 32000 = ~14.16 GiB.
        let mla = KvContext {
            batch_size: 1,
            seq_len: 32_000,
            num_layers: 27,
            num_kv_heads: 0,
            head_dim: 128,
            attention: AttentionKind::MLA,
            kv_latent_dim: 512,
            kv_quant: Dtype::F16,
            ..Default::default()
        };
        let gqa = KvContext {
            attention: AttentionKind::GQA,
            num_kv_heads: 32,
            kv_latent_dim: 0,
            ..mla
        };
        let mla_bytes = estimate_kv_vram(&mla, Dtype::F16);
        let gqa_bytes = estimate_kv_vram(&gqa, Dtype::F16);
        // MLA is at least 4× smaller.
        assert!(mla_bytes * 4 <= gqa_bytes);
        assert_eq!(mla_bytes, 1_769_472_000);
    }

    #[test]
    fn kv_ssm_is_constant_in_seq() {
        let mut ctx = KvContext {
            batch_size: 1,
            seq_len: 0,
            num_layers: 64,
            num_kv_heads: 0,
            head_dim: 0,
            attention: AttentionKind::SSM,
            state_dim: 128,
            kv_quant: Dtype::F16,
            ..Default::default()
        };
        let s_0 = estimate_kv_vram(&ctx, Dtype::F16);
        ctx.seq_len = 1_000_000;
        let s_huge = estimate_kv_vram(&ctx, Dtype::F16);
        // Mamba-2: 64 * 128 * 2 = 16_384 bytes (state only, no KV).
        assert_eq!(s_0, s_huge);
        assert_eq!(s_0, 16_384);
    }

    #[test]
    fn kv_sliding_bounded_by_window() {
        let mut ctx = KvContext {
            batch_size: 1,
            seq_len: 1000,
            num_layers: 32,
            num_kv_heads: 8,
            head_dim: 128,
            attention: AttentionKind::SLIDING,
            window_size: 4096,
            kv_quant: Dtype::F16,
            ..Default::default()
        };
        let at_1k = estimate_kv_vram(&ctx, Dtype::F16);
        ctx.seq_len = 1_000_000;
        let at_huge = estimate_kv_vram(&ctx, Dtype::F16);
        // Window-bound, not seq-bound.
        assert_eq!(at_1k, at_huge);
    }
}

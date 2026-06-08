# 0004 — Local embeddings (fastembed) — future

> Status: **Proposed / Future**
> Date: 2026-06-08
> Deciders: phenoAI maintainers

## Context

`pheno-embedding` currently has a single provider: `OpenAiEmbeddings`, which
requires a network call to OpenAI. Two situations make this problematic:

1. **Air-gapped / offline environments** — cannot reach OpenAI
2. **Cost-sensitive bulk embedding** — re-embedding millions of docs at $0.02/M
   tokens is expensive

`fastembed` is a Rust crate that wraps `ONNX` runtime + a curated set of small
embedding models (BGE-small, all-MiniLM, E5, etc.) that can run on a laptop CPU.

## Decision

**Defer** local embeddings to v0.2. The `pheno-embedding` crate will ship with
only the OpenAI provider in v0.

## Rationale

1. The trait boundary (`EmbeddingProvider`) means local embeddings can be added
   without breaking changes — it's just a new impl.
2. `fastembed` adds a ~50MB ONNX runtime download and increases build complexity
   (system libs). We don't want to bloat the v0 dependency tree.
3. We don't yet have benchmarks showing local embeddings are a hot path for any
   Phenotype service.

## Proposed v0.2 Shape

```rust
// crates/pheno-embedding/src/providers/local.rs
pub struct FastembedEmbeddings {
    model: FastembedModel,
}

#[async_trait]
impl EmbeddingProvider for FastembedEmbeddings {
    async fn embed(&self, req: &EmbeddingRequest) -> Result<EmbeddingResponse, EmbeddingError> {
        // call fastembed synchronously inside spawn_blocking
    }
}
```

Behind a `local` feature flag so consumers opt in.

## When to Revisit

This ADR should be re-evaluated when ANY of these is true:

- A Phenotype service needs to embed in an offline environment
- Bulk embedding cost becomes a measurable line item
- ONNX runtime becomes broadly available via system packages (no build-from-source)

## Consequences

**Positive**
- v0 ships faster, smaller, simpler
- Trait boundary proven before adding second impl
- Local embeddings can be a single focused PR

**Negative**
- v0 forces every consumer to use OpenAI (or roll their own local)
- Tests against the trait are biased toward HTTP-shaped providers

## Cross-References

- `SPEC.md` § "Open Questions"
- `docs/adr/0002-rust-workspace-crate-split.md` — workspace structure
- `PLAN.md` § Backlog

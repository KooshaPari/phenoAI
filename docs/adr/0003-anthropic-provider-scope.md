# 0003 — Anthropic provider scope (deferred)

> Status: **Proposed / Deferred**
> Date: 2026-06-08
> Deciders: phenoAI maintainers

## Context

The Phenotype ecosystem uses Anthropic's Claude family of models extensively.
`phenoAI` currently has only an OpenAI provider in `llm-router`. Several downstream
services (e.g. `thegent`, `heliosLab`) have been blocked or forced to add ad-hoc
shims because there is no first-class Anthropic support.

## Decision

**Defer** the Anthropic provider to the next quarter. Do not block the v0 release
of `phenoAI` on it.

## Rationale

1. The OpenAI-compatible API surface (chat completions, embeddings) is the lowest
   common denominator and covers 80% of immediate needs. Anthropic's API has
   different request/response shapes (separate `system` field handling, `tool_use`
   content blocks, no streaming-completions parity), so it is not a drop-in.
2. v0 should land with a working OpenAI provider + trait abstraction, then
   Anthropic becomes a 200-300 line addition rather than a design surface.
3. The trait boundary (`LlmProvider` / `EmbeddingProvider`) is the right place to
   cut — Anthropic support is a matter of implementing that trait, not changing
   the workspace shape.

## When to Revisit

This ADR should be re-evaluated when ANY of these is true:

- A second downstream service requests Anthropic support
- A Phenotype-wide adoption agreement with Anthropic is signed
- The first Anthropic tool-use content block is needed by an existing service

When revisited, the proposed shape is:

```rust
// crates/llm-router/src/providers/anthropic.rs
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: SecretString,
    base_url: String,
}

#[async_trait]
impl LlmProvider for AnthropicProvider { /* … */ }
```

with the `pheno-llm-router` `Cargo.toml` adding `anthropic` as an optional
feature flag so consumers opt in.

## Consequences

**Positive**
- Faster v0 shipping
- Trait boundary proven before adding a second implementation
- Anthropic work can be a single, focused PR later

**Negative**
- Downstream services that want Anthropic today must roll their own
- Some service duplication of Anthropic glue code (until phenoAI catches up)

## Cross-References

- `SPEC.md` § "Open Questions"
- `docs/adr/0002-rust-workspace-crate-split.md` — workspace structure
- `PLAN.md` § Backlog

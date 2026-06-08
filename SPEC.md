# phenoAI — Specification

> Status: **Living document** — updated as the workspace evolves.
> Last updated: 2026-06-08

## Purpose

`phenoAI` is a Rust workspace providing inference-time primitives for the Phenotype
ecosystem: a multi-provider LLM router, a Model-Context-Protocol (MCP) server, and
an embedding pipeline. The workspace is designed to be embedded into larger Phenotype
services as foundational AI plumbing.

## Workspace Layout

```
phenoAI/
├── crates/
│   ├── llm-router/        # Multi-provider LLM routing (OpenAI, Anthropic, …)
│   ├── mcp-server/        # Model Context Protocol server (tools/resources/prompts)
│   └── pheno-embedding/   # Unified embedding interface (OpenAI, …)
├── docs/
│   ├── adr/               # Architecture Decision Records
│   ├── operations/        # Operational runbooks
│   └── worklogs/          # Dated worklog entries
├── tests/                 # Cross-crate integration tests
├── FUNCTIONAL_REQUIREMENTS.md
├── AGENTS.md              # Agent operating instructions
└── SPEC.md                # This file
```

## Design Principles

1. **Embeddable, not standalone** — `phenoAI` is consumed as a library by other
   Phenotype services; there is no primary binary.
2. **Async by default** — every public API is `async` and uses `tokio` runtime.
3. **Provider-agnostic** — concrete providers (OpenAI, Anthropic) are behind traits
   so a new provider can be added without modifying call-sites.
4. **Tested at the trait boundary** — mocks implement the same trait as the real
   provider, so unit tests do not require network calls.
5. **Bounded retries** — exponential backoff with a hard cap; never loop forever.

## Crate Contracts

### `llm-router`

- **Role**: Route completion requests to one of N configured providers, with
  fallback semantics.
- **Public surface** (high-level): `LlmRouter::route`, `LlmRouter::add_provider`.
- **Errors**: typed `LlmError` enum, not raw `anyhow::Error` at the boundary.
- **Tests**: at least one test per provider mock; one test for fallback path.

### `mcp-server`

- **Role**: Implement the Model Context Protocol (MCP) so that any MCP-compatible
  client can call tools / read resources exposed by Phenotype services.
- **Public surface**: `McpServer::register_tool`, `McpServer::call_tool`,
  `McpServer::register_resource`, `McpServer::read_resource`.
- **Errors**: `McpError` enum (`ToolNotFound`, `ResourceNotFound`, `InvalidRequest`).
- **Tests**: tool round-trip, resource read, error path for unknown tool.

### `pheno-embedding`

- **Role**: Embedding generation. `OpenAiEmbeddings` is the v0 implementation.
- **Public surface**: `OpenAiEmbeddings::embed(&EmbeddingRequest) -> Result<EmbeddingResponse>`.
- **Errors**: `EmbeddingError` enum (`Provider`, `InvalidInput`).
- **Tests**: request shape, default model selection, error mapping.

## Cross-Cutting Concerns

- **Logging**: `tracing` crate; structured fields only.
- **Configuration**: `config` crate or env vars; no secret in source.
- **Telemetry**: OpenTelemetry-compatible; behind feature flag `telemetry`.

## Test & Coverage Governance

- **Coverage floor**: 60% line coverage per crate; enforced by
  `tarpaulin.toml` and surfaced via Codecov.
- **Coverage report**: posted as PR comment and archived as
  `tarpaulin-report.html` artifact.
- **BDD**: at least one `.feature` file per crate covering the happy-path
  user journey (see `tests/features/`).
- **CI matrix**: tests must pass on stable + beta; clippy must pass with
  `-D warnings`.

## Open Questions

- Should we add an `anthropic` provider to `llm-router`? (Tracked in
  `docs/adr/0003-anthropic-provider-scope.md`.)
- Should `pheno-embedding` add a `LocalEmbeddings` provider (e.g. `fastembed`)? (Tracked
  in `docs/adr/0004-local-embeddings-future.md`.)

## Cross-References

- `FUNCTIONAL_REQUIREMENTS.md` — high-level FRs and acceptance criteria
- `AGENTS.md` — agent operating instructions
- `docs/adr/0001-record-architecture-decisions.md` — ADR template
- `docs/adr/0002-rust-workspace-crate-split.md` — why 3 crates
- `docs/adr/0003-anthropic-provider-scope.md` — pending
- `docs/adr/0004-local-embeddings-future.md` — pending

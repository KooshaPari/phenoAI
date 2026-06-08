# 0002 — Rust workspace crate split

> Status: **Accepted**
> Date: 2026-06-08
> Deciders: phenoAI maintainers

## Context

`phenoAI` is a multi-purpose Rust workspace providing AI infrastructure primitives
to the broader Phenotype ecosystem. The first three candidate capabilities are:

1. Multi-provider LLM routing
2. Model Context Protocol (MCP) server
3. Embedding generation

A common mistake in early-stage Rust libraries is to ship these as a single crate,
which forces every downstream consumer to pull in all three capabilities (and their
respective dependency trees) even when they only need one.

## Decision

Split `phenoAI` into a workspace of three independent crates:

| Crate | Responsibility | Reused deps |
|---|---|---|
| `llm-router` | LLM completion routing across providers | `reqwest`, `serde`, `tokio` |
| `mcp-server` | MCP server framework (tools / resources / prompts) | `serde`, `tokio`, `tracing` |
| `pheno-embedding` | Embedding generation (OpenAI first) | `reqwest`, `serde` |

A downstream service that only needs MCP server gets a 2-dep crate, not a 6-dep
monolith. The `Cargo.toml` at the workspace root simply lists them as members and
shares `[workspace.dependencies]`.

## Consequences

**Positive**
- Smaller dependency footprint for partial consumers
- Independent versioning if any crate stabilizes faster than others
- Build parallelism (`cargo build -p mcp-server` doesn't compile llm-router)
- Cleaner cognitive scope: a contributor working on `pheno-embedding` doesn't need
  to know about LLM router internals

**Negative**
- More boilerplate (3 × `Cargo.toml`, 3 × `lib.rs`)
- Cross-crate types must be re-exported through workspace `pub use`
- Test utilities must be duplicated (or extracted to a `test-utils` member if
  reuse grows)

**Mitigations**
- Keep each `Cargo.toml` minimal — no `default-features` for big deps
- Reserve a `pheno-ai-test-utils` member if duplication becomes painful

## Alternatives Considered

1. **Single crate** — rejected; amplifies the "pull in everything" problem.
2. **Two crates** (one for LLM+embed, one for MCP) — rejected; LLM and embedding
   are independent capabilities that benefit from being separately versioned.
3. **Feature flags in one crate** — rejected; doesn't help dependency tree pruning
   unless every dep is `optional = true`, which is fragile to maintain.

## Cross-References

- `SPEC.md` § "Workspace Layout" — high-level overview
- `docs/adr/0001-record-architecture-decisions.md` — ADR process

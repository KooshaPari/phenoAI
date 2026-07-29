# phenoAI — Plan

> Living plan. Updated as scope changes.
> Last updated: 2026-06-08

## Current Quarter (2026 Q2 → Q3)

### Completed
- [x] 3-crate Rust workspace skeleton (llm-router, mcp-server, pheno-embedding)
- [x] Trait-based provider abstraction in `llm-router`
- [x] MCP server with tool / resource registration
- [x] OpenAI embedding client
- [x] FUNCTIONAL_REQUIREMENTS.md with personas + FRs
- [x] AGENTS.md operating instructions
- [x] ADR template (0001)
- [x] 1 ADR filled (record architecture decisions)

### In Progress
- [x] **Coverage governance configuration** — `.codecov.yml`, `tarpaulin.toml`,
      and `.github/workflows/coverage.yml` are present and committed.
- [x] **Test coverage floor** — `cargo tarpaulin` reports 75.97% (117/154 lines)
- [x] **2 more ADRs** (anthropic provider scope, local embeddings future)
- [x] **BDD feature files** — one happy-path `.feature` exists for each crate:
      `llm-router`, `mcp-server`, and `pheno-embedding`.


### Backlog
- [ ] Anthropic provider in `llm-router`
- [ ] Local embeddings via `fastembed`
- [ ] Streaming response support across all providers
- [ ] Telemetry feature flag (OpenTelemetry)

## Test & Coverage Roadmap

| Crate | Current Tests | Target Tests | Current Coverage | Target Coverage |
|---|---|---|---|---|
| llm-router | 0 | ≥ 5 | 0% | 80% |
| mcp-server | 0 | ≥ 4 | 0% | 80% |
| pheno-embedding | 0 | ≥ 3 | 0% | 70% |

## Governance Roadmap

| Item | Status | Owner |
|---|---|---|
| SPEC.md | ✅ this commit | — |
| PLAN.md | ✅ this commit | — |
| FR.md | ✅ existing | — |
| AGENTS.md | ✅ existing | — |
| ADR template | ✅ 0001 | — |
| 2 more ADRs | ✅ 0003 and 0004 | — |
| Codecov config | ✅ this commit | — |
| Tarpaulin config | ✅ this commit | — |
| Coverage workflow | ✅ this commit | — |
| BDD .feature files | ✅ one per crate | — |

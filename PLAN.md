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
- [ ] **Coverage governance** — `.codecov.yml`, `tarpaulin.toml`, `coverage.yml`
      workflow (this commit)
- [ ] **Test coverage floor** — write unit tests until `cargo tarpaulin` reports ≥ 60%
- [ ] **2 more ADRs** (anthropic provider scope, local embeddings future)
- [ ] **BDD feature files** — one happy-path `.feature` per crate

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
| 2 more ADRs | ⏳ pending | — |
| Codecov config | ✅ this commit | — |
| Tarpaulin config | ✅ this commit | — |
| Coverage workflow | ✅ this commit | — |
| BDD .feature files | ⏳ pending | — |

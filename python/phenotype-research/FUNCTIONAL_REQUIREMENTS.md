# Functional Requirements — phenotype-research-engine

**Version:** 1.0.0
**Traces to:** PRD epics E1–E3

---

## FR-RES-001 — GitHub Connector
**SHALL** implement a `GitHubConnector` that fetches repository contents, README, PRD, FR,
and ADR files using the GitHub REST API with token authentication.
**Traces to:** E1.1

## FR-RES-002 — Web Search Connector
**SHALL** implement a `WebSearchConnector` that queries a configured search API and returns
ranked result snippets with source URLs.
**Traces to:** E1.1

## FR-RES-003 — Doc Corpus Connector
**SHALL** implement a `DocCorpusConnector` that queries the local `phenotype-docs-engine`
search index for relevant documents given a query string.
**Traces to:** E1.1

## FR-RES-004 — Parallel Connector Execution
**SHALL** execute all configured connectors concurrently using `asyncio.gather` so that
total wall-clock time equals the slowest connector, not the sum.
**Traces to:** E1.2

## FR-RES-005 — Connector Timeout Enforcement
**SHALL** enforce a per-connector timeout (default 30 s); a timed-out connector contributes
an empty evidence set and logs a warning rather than failing the pipeline.
**Traces to:** E1.2

## FR-RES-006 — Structured Report Schema
**SHALL** produce research reports conforming to a JSON schema with required fields: `title`,
`summary`, `sections` (array of `{heading, content, citations}`), `confidence` (0.0–1.0),
`generated_at` (ISO 8601).
**Traces to:** E2.1

## FR-RES-007 — Citation Traceability
**SHALL** include a `citations` array in every report section, where each citation contains
`source_url`, `fetched_at`, and `excerpt`.
**Traces to:** E2.2

## FR-RES-008 — Append-Only Evidence Log
**SHALL** write all gathered evidence to an append-only JSONL file (`evidence.log`) with
fields: `source_url`, `content_hash`, `fetched_at`, `connector`.
**Traces to:** E3.1

## FR-RES-009 — Content-Hash Deduplication
**SHALL** skip re-fetching evidence whose `content_hash` appears in the evidence log within
the configured TTL window (default 24 h).
**Traces to:** E3.2

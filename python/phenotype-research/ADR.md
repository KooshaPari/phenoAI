# Architecture Decision Records — phenotype-research-engine

---

## ADR-001 — Hexagonal Ports for Source Connectors

**Date:** 2025-09-01
**Status:** Accepted

### Context
The research pipeline needs to fetch from GitHub API, web search APIs, and local files.
These are all secondary adapters (outbound I/O). Per Phenotype mandate they must be behind ports.

### Decision
Define a `ResearchSource` Protocol:
```python
class ResearchSource(Protocol):
    async def gather(self, query: str) -> list[Evidence]: ...
```
All connectors implement this protocol. The pipeline orchestrator depends only on
`list[ResearchSource]`.

### Consequences
- Tests inject `MockResearchSource` fixtures.
- New sources (S3, Confluence) require only a new adapter class.

---

## ADR-002 — JSONL Append-Only Evidence Log

**Date:** 2025-09-10
**Status:** Accepted

### Context
A relational DB was considered for the evidence store but adds infrastructure complexity for
what is essentially a time-series append log.

### Decision
Use a JSONL (newline-delimited JSON) file as the evidence log. Writes are append-only.
Deduplication uses an in-memory set of `(source_url, content_hash)` pairs loaded at startup.

### Consequences
- No database dependency; log is inspectable with `jq`.
- In-memory dedup index is rebuilt on restart from the log file — acceptable for expected
  log sizes (<100K entries).
- For production-scale deployments, the `EvidenceStore` port can be backed by a DB adapter.

---

## ADR-003 — Confidence Scoring via LLM Self-Assessment

**Date:** 2025-10-01
**Status:** Accepted

### Context
Report consumers (downstream agents, human reviewers) need a signal about how well-evidenced
a research report is. Manual scoring is not feasible in an agent-driven workflow.

### Decision
The synthesis step calls the LLM with a structured prompt asking it to score its own confidence
(0.0–1.0) based on the number and quality of sources. This score is included in the report.

### Consequences
- Confidence scores are LLM-estimated, not mathematically derived; consumers must treat them
  as advisory signals.
- Future improvement: replace LLM self-assessment with a calibrated scoring model.

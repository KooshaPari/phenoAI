# PRD — phenotype-research-engine

**Version:** 1.0.0
**Stack:** Python 3.11+, TypeScript
**Repo:** `KooshaPari/phenotype-research-engine`
**Status:** Archived (2026-03-25) — migrated to `packages/phenotype-research/`

---

## Overview

`phenotype-research-engine` is the research and analysis engine for Phenotype agent workflows.
It orchestrates multi-source evidence gathering (GitHub, web search, internal docs), synthesises
findings into structured research reports, and maintains an evidence log with source provenance.

---

## Epics

### E1 — Multi-Source Research Pipeline
**Goal:** Gather evidence from diverse sources into a unified corpus.

#### E1.1 Source Connectors
- As an agent, I want built-in connectors for GitHub repos, web search, and the internal doc
  corpus so I can gather evidence without custom integration code.
- **Acceptance:** Connectors: `GitHubConnector`, `WebSearchConnector`, `DocCorpusConnector`.

#### E1.2 Parallel Evidence Gathering
- **Acceptance:** All configured connectors run concurrently; total latency = max(connector
  latencies) not sum.

### E2 — Evidence Synthesis
**Goal:** Produce structured, citable research reports.

#### E2.1 Structured Report Generation
- As an agent, I want a structured `ResearchReport` with sections, citations, and confidence
  scores so downstream decision-making is grounded in evidence.
- **Acceptance:** Report schema: `{ title, summary, sections[{heading, content, citations}],
  confidence, generated_at }`.

#### E2.2 Source Provenance
- **Acceptance:** Every claim in a report is traceable to a source URL and fetch timestamp.

### E3 — Evidence Persistence
**Goal:** Evidence logs survive restarts and are queryable.

#### E3.1 Persistent Evidence Log
- **Acceptance:** All gathered evidence is written to an append-only log with source URL,
  content hash, and fetch time; log survives process restart.

#### E3.2 Deduplication
- **Acceptance:** Evidence with the same content hash is not re-fetched within a configurable
  TTL (default 24 h).

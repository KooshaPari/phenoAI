# Architecture

## Overview
- PhenoMCP is a multi-language MCP-oriented workspace with Rust, Go, Python, and TypeScript code.
- The workspace provides search and storage-backed services with shared runtime and bindings.
- This document is a skeleton for the repo's component boundaries and data movement.

## Components
## crates/pheno-meilisearch
- Rust crate for Meilisearch-backed capabilities.

## crates/pheno-qdrant
- Rust crate for Qdrant-backed capabilities.

## crates/phenotype-surrealdb
- Rust crate for SurrealDB-backed capabilities.

## go
- Go packages for supporting services, adapters, or command surfaces.

## python
- Python support code and automation entrypoints.

## ts
- TypeScript integration, client, or documentation-facing assets.

## Data flow
```text
client or tool request -> binding layer -> storage/search crate -> external backend or service
```

## Key invariants
- Keep backend-specific logic isolated inside the relevant crate.
- Preserve a clear separation between bindings and storage providers.
- Treat workspace-level contracts as the source of truth for external callers.

## Cross-cutting concerns (config, telemetry, errors)
- Config: define backend endpoints and runtime options centrally.
- Telemetry: keep logging and tracing consistent across language boundaries.
- Errors: translate backend failures into stable workspace-level error shapes.

## Future considerations
- Expand the component map to the concrete package list as ownership settles.
- Add sequence diagrams for query, indexing, and adapter flows.
- Capture testing and deployment assumptions for each backend provider.

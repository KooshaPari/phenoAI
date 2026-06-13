# Architecture

## Overview
- phenoAI is a Rust workspace in the Phenotype ecosystem focused on AI-agent capabilities.
- Stack: Rust (Cargo workspace), Python bindings, with shared Phenotype crates.
- This document is a skeleton; expand with crate-level ownership and boundaries as the workspace evolves.

## Components
## crates/*
- Rust crates implementing the AI agent domain logic, model integrations, and shared utilities.

## python/
- Python bindings and helper modules that wrap the Rust crates for ecosystem consumers.

## ports/
- External port surfaces (CLI, MCP, library) that expose the workspace to other Phenotype projects.

## docs/
- Project documentation: guides, reports, research, reference, checklists (per `AGENTS.md` layout).

## Data flow
```text
external caller (CLI / MCP / Python)
  -> ports/ entrypoint
  -> crates/* domain logic
  -> python/ bindings (optional)
  -> external model / storage providers
```

## Key invariants
- Treat crate boundaries as explicit contracts; respect `deny.toml` and `lefthook.yml` quality gates.
- Mirror the Phenotype org scripting policy (Rust default; no new shell scripts).
- Functional requirements in `FUNCTIONAL_REQUIREMENTS.md` MUST be traceable to at least one test.
- Cross-project reuse: prefer extraction into existing shared modules (see `AGENTS.md`).

## Cross-cutting concerns (config, telemetry, errors)
- Config: load via Phenotype shared config patterns.
- Telemetry: propagate structured logs and traces across the workspace.
- Errors: normalize failure handling so port surfaces can report actionable messages.

## Future considerations
- Replace this skeleton with per-crate ownership and a concrete call-graph diagram.
- Capture release and integration assumptions as the workspace evolves (see `PLAN.md`).
- Add startup diagrams for CLI, MCP, and Python-binding paths.

# Research Worklog

<!-- Research entries document exploratory work, feasibility studies, and abandoned branches. -->

### 2026-06-18 | RESEARCH | Rust LSP scaffolding via uniffi explored

**Context:** MCPForge exploration of a Rust performance layer for LSP protocol handling.
**Finding:** Built scaffolding for `mcp-forge-lsp` crate with tokio + lsp-server + lsp-types + uniffi. Abandoned due to 13 compile errors in Rust layer.
**Decision:** Go pathway retained as primary. Rust layer deprioritized pending resolution of compile errors and uniffi CLI integration complexity.
**Impact:** -257 lines of experimental code. Decision informs future Rust fork strategy.
**Tags:** `[cross-repo]` `[RESEARCH]` `[MCPForge]`

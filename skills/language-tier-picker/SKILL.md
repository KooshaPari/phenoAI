# Language tier picker

Use when adding MCP code and choosing a language or repo layer.

## Pre-flight

1. [LANGUAGE-TIERS-AND-ROLES.md](../../docs/LANGUAGE-TIERS-AND-ROLES.md)
2. [ADR-017](https://github.com/KooshaPari/PhenoSpecs/blob/main/adrs/017-mcp-polyrepo-boundaries.md)
3. [catalog/registry.yaml](../../catalog/registry.yaml) `language_policy`

## Tier rules

| Tier | Languages | MCP role |
|------|-----------|----------|
| 0 | Rust, Zig, Mojo | Framework core, spec-adjacent runtimes |
| 1 | Go | Edge servers (MCPForge; ops-mcp currently deprecated) — requires EDGE-JUSTIFICATION |
| 2 | Python, TypeScript, C#, Java | Framework bindings, IDE plugins |
| 3 | Shell, pwsh | Tooling scripts only |

## Decision tree

- **Protocol core / macros / server builder** → tier 0 framework repo (PhenoFastMCP-rust, not phenotype-rust-sdk)
- **Deployable MCP server with external API** → tier 1 Go only with justification doc
- **FastMCP-style DX / agents** → tier 2 Python (PhenoFastMCP)
- **Fleet dispatch / argv / HTTP** → substrate (not a language-tier repo)
- **Catalog server entry** → PhenoMCPServers/servers/

## Anti-patterns

- `phenotype-go-sdk` or `phenotype-rust-sdk` as MCP home
- Tier-1 Go without `docs/EDGE-JUSTIFICATION.md`
- Zig/Mojo implementation before ADR-020 gates clear

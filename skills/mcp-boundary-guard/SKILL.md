# MCP boundary guard

Use before forking, renaming, or placing MCP-related code.

## Pre-flight (required)

1. [ADR-017](https://github.com/KooshaPari/PhenoSpecs/blob/main/adrs/017-mcp-polyrepo-boundaries.md)
2. [catalog/registry.yaml](https://github.com/KooshaPari/PhenoMCPServers/blob/main/catalog/registry.yaml)
3. Target repo `PHENO.md` + `FORK-NOTES.md`

## Decision tree

### Where does this code go?

- **Framework SDK** (transport, macros, CLI) → PhenoFastMCP-{py,go,rust} or **PhenoRMCP** (spec only)
- **Deployable MCP server/tool** → PhenoMCPServers/servers/
- **Fleet dispatch / argv / HTTP driver** → substrate/
- **Cursor skill / plugin / agent** → PhenoMCPServers/{skills,plugins,agents}/
- **Project template** → HexaKit/ (scaffold only)

### Which Rust MCP fork?

| Need | Repo | Parent |
|------|------|--------|
| fastmcp ergonomics (macros, server builder) | PhenoFastMCP-rust | Dicklesworthstone/fastmcp_rust |
| Official rmcp / streamable HTTP / OAuth / spec tests | PhenoRMCP | modelcontextprotocol/rust-sdk |

**Never** use rmcp under the PhenoFastMCP-rust name.

## Anti-patterns

- `phenotype-rust-sdk` or `phenotype-go-sdk` as MCP framework
- McpKit as lib warehouse
- Mirror-push into empty GitHub repo
- cheap-llm MCP server repo (substrate driver-argv only)

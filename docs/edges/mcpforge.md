# EDGE-JUSTIFICATION — MCPForge

MCPForge is now retired from this catalog and is no longer maintained as an active edge server.

**Catalog ID:** `mcpforge` (archived) | **Absorbed by:** PhenoFastMCP-go

MCPForge historical context remains in [PhenoFastMCP-go docs](https://github.com/KooshaPari/PhenoFastMCP-go/blob/main/SUPERSET.md).  
Operational ownership of MCP edge behavior now routes through PhenoFastMCP-go and the
PhenoMCPServers substrate boundary.

## ADR-019 placement note

MCPForge LSP protocol-generation and semantic tooling patterns are treated as **tier-1 edge**
behavior in ADR-019.

- Runtime and sync source of truth: `servers/substrate/` (PhenoMCPServers catalog path)
- Substrate dev mirror: `KooshaPari/substrate` (`driver-mcp` mirror; see `driver-mcp/SYNC.md`)


# Retired MCP repos — routing table

**Status:** active reference (ADR-017/019)  
**SSOT map:** [phenotype-registry ECOSYSTEM_MAP](https://github.com/KooshaPari/phenotype-registry/blob/main/ECOSYSTEM_MAP.md)

| Retired repo | Status | Use instead |
|--------------|--------|-------------|
| [PhenoMCP](https://github.com/KooshaPari/PhenoMCP) | archived 2026-06-17 | [PhenoMCPServers](https://github.com/KooshaPari/PhenoMCPServers) `servers/` + [PhenoFastMCP](https://github.com/KooshaPari/PhenoFastMCP)* |
| [McpKit](https://github.com/KooshaPari/McpKit) | deleted 2026-06-19 (GH repo no longer resolves) | PhenoFastMCP-py + PhenoMCPServers catalog + PhenoFastMCP-rust (rust/) |
| [MCPForge](https://github.com/KooshaPari/MCPForge) | deprecated 2026-06-18 | [PhenoFastMCP-go](https://github.com/KooshaPari/PhenoFastMCP-go) (absorbed) |
| cheap-llm standalone MCP | deleted | [substrate](https://github.com/KooshaPari/substrate) `driver-argv` only |
| dispatch-mcp / thegent-dispatch | deleted | substrate `driver-http` + `servers/substrate/dispatch_server.py` |
| [phenotype-ops-mcp](https://github.com/KooshaPari/phenotype-ops-mcp) | deprecated 2026-06-18 | migration lane in progress (ADR-019 routing consolidation) |
| phenotype-go-sdk (MCP bucket) | shrink per [#7](https://github.com/KooshaPari/PhenoMCPServers/issues/7) | PhenoFastMCP-go (active tier-1), plus MCPForge and phenotype-ops-mcp deprecation lanes |
| phenotype-rust-sdk (MCP bucket) | retired anti-pattern | PhenoFastMCP-rust + PhenoRMCP |
| [dagctl](https://github.com/KooshaPari/dagctl) | archived 2026-06-17 | [phenodag](https://github.com/KooshaPari/phenodag) v1.0.0-rc.1+ |
| [Apisync](https://github.com/KooshaPari/Apisync) | **DELETED** 2026-06-20 (was ARCHIVED 2026-06-19, fully drained 2026-06-20 — all 153 source files absorbed, 0 remaining) | absorbs → [apikit](https://github.com/KooshaPari/apikit) @ 28887d6 (HTTP toolkit + final governance/CI/tooling drain) + [HexaKit](https://github.com/KooshaPari/HexaKit) 5 ADRs @ 48a8d4f + [HexaKit `templates/hexagon/rust/`](https://github.com/KooshaPari/HexaKit/tree/main/templates/hexagon/rust) cargo templates @ 31a1332 |
| [pheno-cargo-template](https://github.com/KooshaPari/pheno-cargo-template) | **DELETED** 2026-06-20 (was ABSORBED-INTO-HEXAKIT) | [HexaKit `templates/hexagon/rust/`](https://github.com/KooshaPari/HexaKit/tree/main/templates/hexagon/rust) — use `cargo generate --git https://github.com/KooshaPari/HexaKit --subfolder templates/hexagon/rust` |


## AgentMCP patterns (absorb, not duplicate)

[AgentMCP](https://github.com/KooshaPari/AgentMCP) remains the **agent-runtime** MCP protocol lane (orchestration tools).
Do not re-home AgentMCP into PhenoMCPServers `servers/` — instead:

- Deployable fleet MCP tools → PhenoMCPServers `servers/substrate/`, `servers/pheno-org/`
- Agent orchestration MCP → AgentMCP + Agentora integration (see PhenoSpecs platform spec 016)
- Hexagonal layout for new servers → [HexaKit `mcp-server` template](../HEXAKIT_INTEGRATION.md)

## Validation

- `python scripts/validate_stale_patterns.py` — bans revived bucket repos, unsupported fork bootstraps, legacy mirror bootstrap paths, and other pointer-pattern regressions
- `python scripts/validate_fork_parents.py` — fork_parent must match catalog

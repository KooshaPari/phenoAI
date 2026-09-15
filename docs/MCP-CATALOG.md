# Phenotype MCP Catalog

> Authoritative registry of Phenotype-org MCP servers: what exists, what
> framework/transport each uses, the tool surface it exposes, and how to wire
> it into a Claude/agent client. Maintained as the org's single source of truth
> for the MCP surface. Update this file whenever an MCP server is added,
> renamed, or changes transport/tool surface.

## Servers

| Server | Repo | Lang | Framework | Transport | Status | Tool prefix |
|---|---|---|---|---|---|---|
| dispatch-mcp | KooshaPari/dispatch-mcp | Python | FastMCP 3.x | stdio | Alive | `dispatch_*` |
| cheap-llm (archived) | KooshaPari/cheap-llm | Python | FastMCP 2.x | stdio | **Graduated → PhenoMCP** | (unprefixed) |
| KaskMan (MCP) | KooshaPari/KaskMan | Node/TS | `@modelcontextprotocol/sdk` 1.x | stdio | Alive | `rd-platform-*` |
| kmobile (MCP) | KooshaPari/kmobile | Rust | hand-rolled JSON-RPC | stdio | Alive | `device_*`,`simulator_*`,`project_*`,`test_*`,`mobile_*` |
| MCPForge | KooshaPari/MCPForge | Go | `mark3labs/mcp-go` 0.25 | stdio | Alive | LSP semantic tools (get-definition, references, rename, diagnostics) |
| PhenoMCP | KooshaPari/PhenoMCP | Polyglot (Rust core + Python bridge) | FastMCP bridge over Rust core | stdio (+ streamable-HTTP planned) | Partial — Python FastMCP bridge works; root Rust binary has no transport/registry yet | `agent_*`,`governance_*`,`knowledge_*`,`policy_*`,`session_*`,`workflow_*` |

## Tool surfaces

### dispatch-mcp (FastMCP 3.x, stdio)
`dispatch`, `dispatch_custom`, `dispatch_health`, `dispatch_liveness`.
Delegates to OmniRoute over HTTP (`OMNIROUTE_URL`).

### cheap-llm MCP bridge (FastMCP 2.x, stdio) — GRADUATED
`complete`, `stream_complete`, `health`, `cost_summary`, `providers`,
`list_live_models`. Routes to Minimax/Kimi/Fireworks budget providers.
**NOTE:** The cheap-llm MCP runtime has been graduated into PhenoMCP.
All functionality, provider integrations, and budget routing now live in the
PhenoMCP Python FastMCP bridge. The original repo is archived.

### kmobile (hand-rolled JSON-RPC, stdio)
`device_list/connect/install`, `simulator_list/start/stop`,
`project_build/status`, `test_run/record`, `mobile_deploy/test`.
NOTE: does not use any MCP SDK — custom `McpServer` struct + serde_json.
Schema drift / protocol-version risk. Candidate for migration to `rmcp`.

### KaskMan (official TS SDK, stdio)
R&D-platform tools registered via `ListToolsRequestSchema`/`CallToolRequestSchema`.

### MCPForge (mark3labs/mcp-go, stdio)
Exposes LSP semantic operations as MCP tools. Distinct purpose (dev tooling),
not an application server.

### PhenoMCP (FastMCP bridge, stdio) — CANONICAL org entrypoint
Org governance/agent/knowledge/policy/session/workflow tools, registered via
`FastMCP.add_tool`. **The Python FastMCP bridge (`python -m pheno_mcp`) is the
canonical, supported PhenoMCP MCP entrypoint.** FastMCP is the org framework
convention and the bridge is the only functional surface today. The Rust root
binary is **experimental/future** — it has no transport, tool registry, resource
provider, or auth layer (see its README "What does not exist yet"). All NEW org
MCP tools land in the FastMCP bridge, not the Rust root.

## Tool-naming standard (NORMATIVE)

All Phenotype MCP tools MUST follow `<server>_<verb>_<noun>` so tools stay
collision-free when servers are aggregated under one client:

- `<server>` — short server slug (`dispatch`, `kaskman`, `kmobile`,
  `mcpforge`, `pheno`).
- `<verb>` — action (`list`, `get`, `create`, `delete`, `run`, `complete`,
  `build`, `health`).
- `<noun>` — target (`device`, `provider`, `agent`, `cost`), singular.

Examples: `dispatch_run_task`, `kmobile_list_device`.
Domain-prefixed names already in use (`agent_*`, `governance_*`, `device_*`) are
grandfathered but SHOULD migrate.

## Client wiring (`.mcp.json` / Claude)

```jsonc
{
  "mcpServers": {
    "dispatch":  { "command": "dispatch-mcp" },
    "kaskman":   { "command": "node", "args": ["src/interfaces/mcp/server.js"] },
    "kmobile":   { "command": "kmobile-mcp" },
    "mcpforge":  { "command": "mcpforge", "args": ["--workspace", "."] },
    "phenomcp":  { "command": "python", "args": ["-m", "pheno_mcp"] }
  }
}
```

## Coherence gaps (tracked)

1. **No shared framework.** Python on FastMCP (good, org convention), but Go on
   mark3labs, TS on official SDK, Rust hand-rolled. Python servers are the
   conformant baseline.
2. **Tool-naming standard** — RESOLVED: `<server>_<verb>_<noun>` org-wide (see
   normative section above). All active servers are now conformant.
3. **kmobile hand-rolled JSON-RPC** — highest protocol-drift risk; migrate to
   `rmcp` (Rust MCP SDK).
4. **PhenoMCP root binary is a shell** — RESOLVED: the Python FastMCP bridge is
   declared the canonical org entrypoint (see above); the Rust root is
   experimental/future. New tools land in the bridge.
5. **Transport is stdio everywhere** — that IS consistent (good). Streamable-HTTP
   only planned in PhenoMCP.

## Verdict

Transport is uniformly stdio (coherent). Framework + tool-naming are NOT
coherent — three SDKs plus two hand-rolled implementations, three naming
conventions. The Python pair (FastMCP) is the conformant core; the path to
coherence is: (a) this catalog as the registry, (b) org tool-naming standard,
(c) migrate kmobile to rmcp, (d) resolve PhenoMCP's bridge-vs-root-binary split.

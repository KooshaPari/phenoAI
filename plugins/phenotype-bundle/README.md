# Phenotype default MCP bundle

Merge `mcp.json` into Cursor MCP settings with this repo as cwd.

## Servers

| ID | Role |
|----|------|
| `substrate` | Fleet dispatch + team mailbox (requires `substrate-http` on :8080) |
| `substrate-dispatch` | OmniRoute tier (optional `OMNIROUTE_URL`) |
| `pheno-org` | Governance / agent / workflow tools |

## Optional (pointer catalog entries)

| ID | Source | Notes |
|----|--------|-------|
| `agileplus-mcp-intent` | AgilePlus monorepo `crates/agileplus-mcp-intent` | Rust MCP; wire when binary path is pinned — see [AgilePlus docs/mcp/INTEGRATION.md](https://github.com/KooshaPari/AgilePlus/blob/main/docs/mcp/INTEGRATION.md) |

Fleet-lead agent references catalog pointers; extend `mcp.json` when agileplus binary is fleet-stable.

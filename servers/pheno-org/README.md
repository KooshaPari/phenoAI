# Pheno-org MCP server

Native PhenoMCPServers FastMCP server for the six org tool groups. The surface
was originally migrated from [PhenoMCP](https://github.com/KooshaPari/PhenoMCP)
(`python/src/pheno_mcp/tools/`):

| Group | Tools |
|-------|-------|
| governance | `ledger_query`, `ledger_verify` |
| agent | `agent_create`, `agent_list`, `agent_get`, `agent_delete` |
| knowledge | `knowledge_store`, `knowledge_retrieve`, `knowledge_search`, `knowledge_delete` |
| policy | `policy_list`, `policy_get`, `policy_evaluate` |
| session | `session_suspend`, `session_resume` |
| workflow | `workflow_execute`, `workflow_status`, `workflow_cancel`, `workflow_list` |

All tools proxy to a Parpoura HTTP backend via `httpx.AsyncClient`.

Framework: [PhenoFastMCP](https://github.com/KooshaPari/PhenoFastMCP) / fastmcp 3.4.2.

```bash
cd servers/pheno-org
pip install -e .
export PARPOURA_BASE_URL=http://127.0.0.1:8001
python pheno_org_server.py
```

## Environment

- `PARPOURA_BASE_URL` — Parpoura API base URL (default `http://localhost:8001`)

## Tests

```bash
pip install -e ".[dev]"
pytest
```

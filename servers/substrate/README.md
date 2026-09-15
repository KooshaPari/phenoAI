# Substrate MCP servers

Lifted from `substrate/driver-mcp` (integration branch). Implements:

| Module | Catalog ID |
|--------|------------|
| `substrate_server.py` | `substrate` |
| `dispatch_server.py` | `substrate-dispatch` |
| `lead_server.py` | (mailbox variant) |
| `team_mailbox_server.py` | (worker mailbox) |

Framework: [PhenoFastMCP](https://github.com/KooshaPari/PhenoFastMCP) / fastmcp 3.4.2.

```bash
cd servers/substrate
pip install -r requirements.txt
export SUBSTRATE_HTTP_URL=http://127.0.0.1:8080
python substrate_server.py
```

Substrate runtime (HTTP, argv, cheap-llm CLI) remains in the **substrate** repo.

## driver-mcp mirror sync (ADR-019)

Canonical source is this directory (`servers/substrate/`). The **substrate** repo keeps a dev mirror at
[`driver-mcp/`](https://github.com/KooshaPari/substrate/tree/main/driver-mcp) — see
[`driver-mcp/SYNC.md`](https://github.com/KooshaPari/substrate/blob/main/driver-mcp/SYNC.md).

CI runs `substrate/scripts/check_driver_mcp_sync.py` on every catalog workflow (substrate checked out as sibling).

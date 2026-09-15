# Client wiring

Generate `.mcp.json` snippets from `catalog/registry.yaml` or use bundled plugins.

## Substrate server

```json
{
  "mcpServers": {
    "substrate": {
      "command": "python",
      "args": ["servers/substrate/substrate_server.py"],
      "env": {
        "SUBSTRATE_HTTP_URL": "http://127.0.0.1:8080",
        "SUBSTRATE_TEAM_ID": "default",
        "SUBSTRATE_AGENT_NAME": "lead"
      }
    }
  }
}
```

## Plugin bundle

Copy `plugins/phenotype-bundle/mcp.json` into your project or merge into Cursor MCP settings.

## Skills and agents

- Skills: point Cursor agent skills at `skills/<id>/SKILL.md`
- Agents: load `agents/<id>/agent.yaml` as persona + default server/skill set

## Dogfood

Use [docs/DOGFOOD.md](DOGFOOD.md) for the ADR-018 zero-loop ritual before fleet work.
The fleet lead wiring lives in `agents/fleet-lead/agent.yaml` and
[`agents/fleet-lead/README.md`](../agents/fleet-lead/README.md).

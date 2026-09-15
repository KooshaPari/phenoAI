# HexaKit `mcp-server-ts7` template (planned — PhenoMCPServers target)

> **Status:** `planned` stub (sd-ts7 side-DAG). Not registered in HexaKit CLI yet.
> See [docs/spikes/ts7-hexakit-binding.md](../../docs/spikes/ts7-hexakit-binding.md).

Planned scaffold for:

```bash
hexakit init mcp-server-ts7 --catalog phenomcp --id <my-server>
```

## Difference from `templates/mcp-server/`

| Aspect | `mcp-server` (active) | `mcp-server-ts7` (planned) |
|--------|----------------------|----------------------------|
| Runtime | Python 3.14+ / uv | Bun + TS 7 preview |
| Framework | fastmcp | `@modelcontextprotocol/server` (v2) |
| Server entry | `{{module}}.py` | `src/index.ts` |
| Tier | 2 (binding) | 2 (binding) |

Skill, plugin, and agent companions reuse `templates/mcp-server/` until this lane activates.

## Variables (planned)

Same substitution keys as the Python template: `{{id}}`, `{{title}}`, `{{description}}`,
`{{transport}}`, `{{env}}`, `{{env_list}}`, `{{env_json}}`.

## Local preview (stub renderer)

```bash
python templates/mcp-server-ts7/scripts/render.py \
  --id echo --title "Echo MCP" --description "Echoes input back" \
  --env ECHO_TOKEN --out /tmp/phenomcp-ts7-render
```

The stub renderer writes `server.ts` + `package.json` + catalog entry only. Full
`validate_catalog.py` integration is deferred (see spike doc gate G4).

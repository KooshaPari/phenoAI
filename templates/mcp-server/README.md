# HexaKit `mcp-server` template (PhenoMCPServers target)

Drop-in scaffold for `hexakit init mcp-server --catalog phenomcp`.

The `hexakit` CLI consumes this directory tree and renders a new MCP server
plus its companion skill / plugin / agent into a target PhenoMCPServers
checkout. All rendered artifacts satisfy `scripts/validate_catalog.py`:

- `servers/<id>/<id>_server.py` is the runnable MCP server
- `servers/<id>/pyproject.toml` declares the `phenomcp-server-<id>` package
- `skills/<id>-skill/{SKILL.md,skill.yaml}` is the companion agent skill
- `plugins/<id>-bundle/{plugin.yaml,mcp.json}` wires the server into IDE clients
- `agents/<id>-lead/agent.yaml` is the default agent persona
- `catalog/registry.yaml` gains a `servers[].id=<id>`, `skills[].id=<id>-skill`,
  `plugins[].id=<id>-bundle`, and `agents[].id=<id>-lead` entry

## Variables substituted at render time

| Var | Example | Purpose |
|-----|---------|---------|
| `{{id}}` | `echo` | Stable ID used in `id`, package, paths |
| `{{title}}` | `Echo MCP` | Human-readable title |
| `{{description}}` | `Echoes input back` | One-line summary |
| `{{transport}}` | `stdio` | `stdio` / `sse` / `streamable-http` |
| `{{framework}}` | `fastmcp` | MCP framework, defaults to `fastmcp` |
| `{{module}}` | `echo_server` | Python module name (default `<id>_server`) |
| `{{callable}}` | `mcp.run` | Entry callable |
| `{{env}}` | `[ECHO_TOKEN]` | Comma-separated env var list |

## Directory layout

```
templates/mcp-server/
├── README.md           # this file
├── server.py.tmpl      # → servers/<id>/<module>.py
├── pyproject.toml.tmpl # → servers/<id>/pyproject.toml
├── SKILL.md.tmpl       # → skills/<id>-skill/SKILL.md
├── skill.yaml.tmpl     # → skills/<id>-skill/skill.yaml
├── plugin.yaml.tmpl    # → plugins/<id>-bundle/plugin.yaml
├── mcp.json.tmpl       # → plugins/<id>-bundle/mcp.json
├── agent.yaml.tmpl     # → agents/<id>-lead/agent.yaml
├── catalog_entry.yaml.tmpl  # → appended into catalog/registry.yaml
└── scripts/
    └── render.py       # local renderer used by tests
```

`render.py` substitutes the variables above and validates the resulting
catalog entry shape against `schemas/registry.schema.json`.

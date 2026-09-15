# HexaKit integration

## Python template (active)

The `templates/mcp-server/` directory is a [HexaKit](https://github.com/KooshaPari/HexaKit)
template targeting this catalog. HexaKit's `hexakit init` consumes it via:

```bash
hexakit init mcp-server --catalog phenomcp --id <my-server>
```

`scripts/render.py` inside the template is the local-rendering counterpart
HexaKit invokes; running it produces a layout that passes
`scripts/validate_catalog.py` end-to-end.

## What gets generated

For a single `--id echo` invocation:

| Path | Purpose |
|------|---------|
| `servers/echo/echo_server.py` | Runnable `fastmcp` server with one tool |
| `servers/echo/pyproject.toml` | `phenomcp-server-echo` Python package |
| `servers/echo/README.md` | Quick-start usage |
| `skills/echo-skill/SKILL.md` | Agent skill frontmatter + body |
| `skills/echo-skill/skill.yaml` | Skill manifest (id, path, entry) |
| `plugins/echo-bundle/plugin.yaml` | IDE plugin manifest |
| `plugins/echo-bundle/mcp.json` | Cursor/Claude `mcpServers` snippet |
| `agents/echo-lead/agent.yaml` | Default agent persona |
| `catalog/registry.yaml` | Appends `servers`/`skills`/`plugins`/`agents` entries |

## Validation

After rendering, run:

```bash
python scripts/validate_catalog.py
```

The validator checks two things:

1. **Schema** — every rendered entry conforms to
   `schemas/registry.schema.json` (Draft 2020-12).
2. **Path existence** — `active` entries must point to real files. Scaffolded
   entries use `status: template`, so the path-existence check is skipped
   until you flip the entry to `active` after filling in the server.

## Trying the template locally

```bash
python templates/mcp-server/scripts/render.py \
  --id echo --title "Echo MCP" --description "Echoes input back" \
  --env ECHO_TOKEN --out /tmp/phenomcp-render
ls /tmp/phenomcp-render
```

To verify the rendered entries pass the catalog validator, copy them into
a real PhenoMCPServers checkout and re-run the validator (see
`tests/test_hexakit_template.py::test_validate_catalog_script_passes` for
the exact recipe).

## Promoting template entries to active

1. Implement the server's tools in `servers/<id>/<id>_server.py`.
2. Add a `README.md` and at least one pytest under `servers/<id>/tests/`.
3. Edit `catalog/registry.yaml` and change the entry's `status` from
   `template` to `active`.
4. Re-run `python scripts/validate_catalog.py` — the path-existence check
   now applies.

## TS7 / Bun template (planned — sd-ts7)

| Field | Value |
|-------|-------|
| Template path | `templates/mcp-server-ts7/` |
| Registry lane | `framework.typescript` (`status: planned`) |
| Side-DAG | `sd-ts7` (phenodag `mcp-fleet-60`) |
| Spike doc | [docs/spikes/ts7-hexakit-binding.md](spikes/ts7-hexakit-binding.md) |
| ADR (draft) | PhenoSpecs ADR-021 — tier-2 TS7 binding + HexaKit scaffold |
| Defer gate | MCP TS SDK v2 stable (2026-07-28 RC) + `validate_catalog` CI for TS7 template |

Planned invocation (not wired in HexaKit CLI yet):

```bash
hexakit init mcp-server-ts7 --catalog phenomcp --id <my-server>
```

The stub renderer writes Bun + `@modelcontextprotocol/server` server files only.
Skill / plugin / agent companions reuse the Python `mcp-server` template until the
TS7 lane activates. **Do not create `PhenoFastMCP-ts` or promote the lane to
`status: active` until the defer gate in the spike doc clears.**

# MCP fleet dogfood ritual (ADR-018)

Run once per execution session before claiming parallel lanes.

## Pre-flight (mandatory)

1. Read [ADR-017](https://github.com/KooshaPari/PhenoSpecs/blob/main/adrs/017-mcp-polyrepo-boundaries.md)
2. Read `catalog/registry.yaml` for target server/plugin IDs
3. Read target repo `PHENO.md` + `FORK-NOTES.md`
4. `phenodag pick --agent cursor --db MCP_FLEET.db` (or your agent name)

## Fleet-lead session (eco-027)

**Goal:** `outcome: zero_loop` — no user correction loops.

### Wiring under test

| Artifact | Path |
|----------|------|
| Agent | `agents/fleet-lead/agent.yaml` |
| Plugin bundle | `plugins/phenotype-bundle/mcp.json` |
| Skills | `mcp-boundary-guard`, `github-fork-policy`, `substrate-dispatch-skill` |
| Servers | `substrate`, `substrate-dispatch`, `pheno-org` |

### Runtime prerequisites

```bash
# Terminal 1 — substrate HTTP
cargo run -p driver-http --bin substrate-http

# Terminal 2 — optional OmniRoute for dispatch tier tools
export OMNIROUTE_URL=http://127.0.0.1:20128
```

### Client config

Merge `plugins/phenotype-bundle/mcp.json` into Cursor MCP settings (repo root as cwd).

### Acceptance

- [ ] `python scripts/validate_catalog.py` green
- [ ] `python scripts/validate_bundle_wiring.py` green
- [ ] Agent loads fleet-lead skills without manual path fixes
- [ ] `substrate_dispatch` / `substrate_plan` reach driver-http
- [ ] Session metrics filed per [session-metrics example](https://github.com/KooshaPari/PhenoSpecs/blob/main/specs/mcp/session-metrics/example.yaml)

## Post-session

```bash
phenodag done --agent cursor --task eco-027 --db MCP_FLEET.db
```

Log any `loop_events` to PhenoSpecs ADR-018 Appendix A if new root causes appear.

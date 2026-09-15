# Spike: TS7 / Bun HexaKit MCP binding

**Side-DAG:** `sd-ts7` (phenodag `mcp-fleet-60`)  
**Status:** spike complete — **defer implementation** until MCP TS SDK v2 + TS 7 toolchain gate clears  
**Date:** 2026-06-17

---

## sd-ts7-01 — Template stub

Planned HexaKit target: `templates/mcp-server-ts7/` (sibling to the Python `mcp-server` template).

| Path | Purpose | Status |
|------|---------|--------|
| `templates/mcp-server-ts7/README.md` | Template contract + defer notice | **stub** |
| `templates/mcp-server-ts7/server.ts.tmpl` | Bun/TS7 stdio MCP server skeleton | **stub** |
| `templates/mcp-server-ts7/package.json.tmpl` | `@phenomcp/server-{{id}}` package manifest | **stub** |
| `templates/mcp-server-ts7/catalog_entry.yaml.tmpl` | Registry append shape (`language: typescript`) | **stub** |
| `templates/mcp-server-ts7/scripts/render.py` | Local renderer (mirrors Python template) | **stub** — no CI test yet |

### Intended invocation (not wired in HexaKit CLI yet)

```bash
hexakit init mcp-server-ts7 --catalog phenomcp --id <my-server>
# local preview:
python templates/mcp-server-ts7/scripts/render.py \
  --id echo --title "Echo MCP" --description "Echoes input back" \
  --env ECHO_TOKEN --out /tmp/phenomcp-ts7-render
```

### Shared companion artifacts

Until the TS7 lane activates, **skill / plugin / agent** scaffolds reuse the existing
`templates/mcp-server/` companions (`SKILL.md.tmpl`, `plugin.yaml.tmpl`, etc.). Only the
**server runtime** differs (Bun + `@modelcontextprotocol/server` instead of Python + fastmcp).

### Stub verdict

| Question | Answer |
|----------|--------|
| Renderable end-to-end? | **No** — stub files document shape only |
| Passes `validate_catalog.py`? | **Not yet** — deferred with lane |
| Blocks Python template? | **No** — parallel directory tree |

---

## sd-ts7-02 — Registry planned entry

Source: [`catalog/registry.yaml`](../../catalog/registry.yaml) (`registry_version: "1.1.1"`)

### `language_policy.parallel_workstreams`

| Stream | In registry | Status |
|--------|-------------|--------|
| `phenofastmcp-ts` | yes | planned (comment) |

### `framework.typescript`

| Field | Value |
|-------|-------|
| repo | `https://github.com/KooshaPari/PhenoFastMCP-ts` |
| upstream | `https://github.com/modelcontextprotocol/typescript-sdk` |
| fork_parent (declared) | `modelcontextprotocol/typescript-sdk` |
| tier / role | 2 / mcp-framework |
| toolchain | TS 7 preview; Bun |
| status | **planned** |
| start_trigger | MCP TS SDK v2 stable + HexaKit `mcp-server-ts7` validates (this spike) |
| GH repo exists | **No** (404 as of 2026-06-17) |

### `templates.hexakit`

| Field | Value |
|-------|-------|
| mcp-server | `templates/mcp-server/` — Python + fastmcp (**active**) |
| mcp-server-ts7 | `templates/mcp-server-ts7/` — Bun + TS7 (**planned stub**) |

### Validation gaps

- `scripts/validate_fork_parents.py` validates `rust`, `rmcp`, `go`, `python` only — **typescript not in NORMATIVE map** (intentional while deferred).
- No catalog CI gate for planned-lane repo existence (correct — repo must not exist prematurely).

---

## sd-ts7-03 — Codegen sketch

### Render pipeline (planned)

```
hexakit init mcp-server-ts7
    │
    ├─► substitute {{id}}, {{title}}, {{description}}, {{transport}}, {{env}}
    │
    ├─► servers/<id>/
    │     ├── src/index.ts          ← server.ts.tmpl
    │     ├── package.json          ← package.json.tmpl
    │     └── tsconfig.json         ← copy from HexaKit templates/typescript/
    │
    ├─► skills/<id>-skill/          ← reuse mcp-server companions
    ├─► plugins/<id>-bundle/        ← reuse mcp-server companions
    ├─► agents/<id>-lead/           ← reuse mcp-server companions
    │
    └─► catalog/registry.yaml       ← append catalog_entry.yaml.tmpl
            status: template
```

### Planned `server.ts.tmpl` surface (v2 SDK)

```typescript
import { McpServer } from "@modelcontextprotocol/server";
import { StdioServerTransport } from "@modelcontextprotocol/server";

const server = new McpServer({ name: "{{id}}", version: "0.1.0" });

server.tool("{{id}}_ping", { message: z.string() }, async ({ message }) => ({
  content: [{ type: "text", text: message }],
}));

const transport = new StdioServerTransport();
await server.connect(transport);
```

### Toolchain pins (planned)

| Pin | Value | Owner |
|-----|-------|-------|
| Runtime | Bun ≥ 1.2.x | HexaKit `templates/typescript/` |
| TS compiler | TS 7 preview (`typescript@next` or pinned preview) | HexaKit |
| MCP SDK | `@modelcontextprotocol/server` v2 (post-2026-07-28 RC) | PhenoFastMCP-ts fork |
| Package manager | `bun` (primary); `pnpm` fallback for IDE-only consumers | PhenoMCPServers CI matrix TBD |

### HexaKit catalog wiring (future)

HexaKit repo registers `phenomcp` catalog entries pointing at this monorepo path:

```yaml
# HexaKit catalogs/phenomcp.yaml (sketch — not committed here)
templates:
  mcp-server:
    path: PhenoMCPServers/templates/mcp-server
    runtime: python
  mcp-server-ts7:
    path: PhenoMCPServers/templates/mcp-server-ts7
    runtime: bun
    status: planned
```

---

## sd-ts7-04 — ADR ref

**Proposed:** PhenoSpecs **ADR-021** — *Tier-2 TS7 / Bun MCP binding + HexaKit scaffold*

> **Status:** Draft stub — not yet filed in PhenoSpecs  
> **Depends on:** ADR-017 (polyrepo boundaries), ADR-018 (fork policy)

### Context

`LANGUAGE-TIERS-AND-ROLES.md` places **TS 7 preview / Bun** at tier 2 for IDE plugins,
web admin, and client wiring. PhenoMCPServers ships a Python HexaKit template today;
a TS7 sibling template enables Bun-native MCP servers without hosting protocol core in TS.

### Decision (draft)

1. **Defer** `PhenoFastMCP-ts` fork and full template CI until MCP TS SDK v2 stabilizes (2026-07-28 RC target).
2. Reserve `framework.typescript` in `catalog/registry.yaml` with `status: planned`.
3. Stub `templates/mcp-server-ts7/` in PhenoMCPServers; wire HexaKit catalog entry only after ADR-021 Accepted.
4. **Never** host protocol core or schema generators in TS when Rust/Zig lanes exist (ADR-017 tier rule).
5. Extend `validate_fork_parents.py` when lane moves to `status: active`.

### Consequences

- PhenoMCPServers catalog remains SSOT for lane metadata and template paths.
- `PhenoFastMCP-ts` created only after ADR-021 Accepted + verified `gh repo fork modelcontextprotocol/typescript-sdk`.
- Python `mcp-server` template remains the default scaffold until TS7 gate clears.

### Normative cross-links

- [ADR-017](https://github.com/KooshaPari/PhenoSpecs/blob/main/adrs/017-mcp-polyrepo-boundaries.md) — polyrepo boundaries; TS = tier 2 binding
- [ADR-018](https://github.com/KooshaPari/PhenoSpecs/blob/main/adrs/018-agent-session-zero-loop-ssot.md) — fork / session policy
- [HEXAKIT_INTEGRATION.md](../HEXAKIT_INTEGRATION.md) — Python template (active) + TS7 stub (planned)
- [HexaKit](https://github.com/KooshaPari/HexaKit) — `@hexakit/typescript` + catalog consumer

---

## sd-ts7-05 — Defer gate recommendation

**Recommendation: KEEP DEFERRED** — do not create `PhenoFastMCP-ts` or enable HexaKit `mcp-server-ts7` in Wave 0.

### Gate checklist (all required before `status: active`)

| # | Gate | Owner | Current |
|---|------|-------|---------|
| G1 | MCP TS SDK v2 stable (2026-07-28 RC validated) | modelcontextprotocol/typescript-sdk | **Open** (v2 pre-alpha) |
| G2 | TS 7 preview pin documented in HexaKit `templates/typescript/` | HexaKit | **Open** |
| G3 | ADR-021 filed and **Accepted** in PhenoSpecs | PhenoSpecs | **Open** (stub only) |
| G4 | `templates/mcp-server-ts7/` passes `validate_catalog.py` in CI | PhenoMCPServers | **Open** |
| G5 | `gh repo fork modelcontextprotocol/typescript-sdk` → `PhenoFastMCP-ts` verified | PhenoMCPServers + fork repo | **Open** |
| G6 | `validate_fork_parents.py` extended for `typescript` lane | PhenoMCPServers CI | **Open** |
| G7 | HexaKit `phenomcp` catalog entry for `mcp-server-ts7` | HexaKit | **Open** |

### Exit criteria (promote to active)

1. All gates G1–G7 green.
2. `registry.yaml`: `framework.typescript` `status: planned` → `status: active`.
3. `PHENO.md` + `FORK-NOTES.md` on `PhenoFastMCP-ts`.
4. phenodag: claim follow-on `sd-ts7-*` tasks or new eco-* substrate stage slots.

### Suggested timeline

| Phase | When | Action |
|-------|------|--------|
| Now (Wave 0) | 2026-06 | This spike doc + template stub; defer fork |
| Pre-RC | Before 2026-07-28 | Pin TS7 + v2 SDK against MCP RC spec |
| Post-gate | After G1+G2 | File ADR-021, create fork, activate TS7 template |

---

## Cross-links

- [HEXAKIT_INTEGRATION.md](../HEXAKIT_INTEGRATION.md) — active Python integration + TS7 planned section
- [LANGUAGE-TIERS-AND-ROLES.md](../LANGUAGE-TIERS-AND-ROLES.md) — TS 7 / Bun tier-2 placement
- [templates/mcp-server/README.md](../../templates/mcp-server/README.md) — active Python template
- [templates/mcp-server-ts7/README.md](../../templates/mcp-server-ts7/README.md) — TS7 stub

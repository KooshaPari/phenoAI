# Spike: tier-0 Zig/Mojo MCP core forks

**Side-DAG:** `sd-zigmojo` (phenodag `mcp-fleet-60`)  
**Issue:** [#8 — parallel: tier-0 Zig/Mojo MCP core forks (future)](https://github.com/KooshaPari/PhenoMCPServers/issues/8)  
**Status:** triage complete — **defer implementation** until rmcp superset gate clears  
**Date:** 2026-06-17

---

## sd-zigmojo-01 — Issue #8 triage

| Field | Value |
|-------|-------|
| Title | parallel: tier-0 Zig/Mojo MCP core forks (future) |
| State | **CLOSED** |
| Labels | `enhancement`, `kilo-triaged`, `kilo-auto-fix` |
| Resolution PR | [#9](https://github.com/KooshaPari/PhenoMCPServers/pull/9) — **MERGED** |

### Issue intent

After the **rmcp superset** in `PhenoFastMCP-rust` stabilizes, evaluate Zig and Mojo as **parallel tier-0 MCP framework lanes**. Each lane gets its own GitHub fork repo under the same fork-parent rules as Rust (ADR-017). Do **not** route through the retired `phenotype-rust-sdk` language umbrella.

### What landed in #9

- `catalog/registry.yaml`: `framework.zig` and `framework.mojo` stubs (`status: future`)
- `docs/LANGUAGE-TIERS-AND-ROLES.md`: trigger table and anti-pattern guard
- Issue closed; no implementation repos or CI work started

### Triage verdict

| Question | Answer |
|----------|--------|
| Blocked? | **Yes** — explicit `start_trigger: rmcp superset stabilizes` |
| Action now? | **Spike doc only** (this file); no fork creation |
| Risk if started early? | Fork-parent drift, duplicate spec work, pre-2026-07-28 RC churn |

---

## sd-zigmojo-02 — Fork parent candidates

Registry currently lists `fork_parent: modelcontextprotocol/rust-sdk` for both lanes. That is a **placeholder** mirroring the PhenoRMCP spec-SDK parent, not a verified native upstream for Zig/Mojo.

### Candidate matrix

| Candidate | Lane | Role | Pros | Cons | Recommendation |
|-----------|------|------|------|------|----------------|
| `modelcontextprotocol/rust-sdk` | zig, mojo | spec reference | ADR-017 aligned; same parent as PhenoRMCP; stable governance | No Zig/Mojo source to fork; would be greenfield port, not `gh repo fork` | **Defer** — use as *spec* anchor only until native upstream exists |
| `muhammad-fiaz/mcp.zig` | zig | native framework | Active community Zig MCP; stdio/HTTP; MIT | Unofficial; protocol ~2025-11-25; not phenotype-owned | **Leading Zig candidate** when gate opens |
| `justrach/mcp-zig` | zig | minimal template | Tiny binary; zero deps | Template, not full SDK; older spec (2025-06-18) | Fallback / reference only |
| `nowex35/mojo_mcp` | mojo | partial server | Native Mojo; tools path proven | 0★; partial spec; academic scope | **Leading Mojo candidate** when gate opens; evaluate before fork |
| `Dicklesworthstone/fastmcp_rust` | zig, mojo | ergonomics | Proven PhenoFastMCP-rust parent | Wrong layer — fastmcp ≠ spec SDK (ADR-017) | **Reject** |
| Greenfield (no upstream) | zig, mojo | phenotype-native | Full control; port from stabilized rmcp superset API | No GitHub fork parent; violates ADR-018 unless documented exception | **Last resort** — requires ADR-020 + FORK-NOTES |

### Planned fork procedure (when gate opens)

Per [skills/github-fork-policy](../../skills/github-fork-policy/SKILL.md):

```bash
gh repo fork <upstream>/<repo> --fork-name PhenoFastMCP-zig   # or -mojo
gh api repos/KooshaPari/PhenoFastMCP-zig --jq '{fork, parent: .parent.full_name}'
```

Update `catalog/registry.yaml` `fork_parent` to match verified parent before `status: active`.

---

## sd-zigmojo-03 — ADR draft stub

**Proposed:** PhenoSpecs **ADR-020** — *Tier-0 Zig/Mojo MCP framework forks*

> **Status:** Draft stub — not yet filed in PhenoSpecs  
> **Depends on:** ADR-017 (polyrepo boundaries), ADR-018 (fork policy)

### Context

Issue #8 and registry v1.1.0 reserve tier-0 parallel lanes for Zig and Mojo MCP frameworks. Official MCP org ships no Zig or Mojo SDK; community implementations exist with varying spec coverage.

### Decision (draft)

1. **Defer** fork creation until the rmcp superset gate (below) is satisfied.
2. When starting Zig: prefer fork of `muhammad-fiaz/mcp.zig` (or successor) after spec audit; update registry `fork_parent`.
3. When starting Mojo: evaluate `nowex35/mojo_mcp` or greenfield port from stabilized PhenoRMCP API surface; document parent in ADR + FORK-NOTES.
4. **Never** add Zig/Mojo to `phenotype-rust-sdk` or any language umbrella bucket.
5. Extend `scripts/validate_fork_parents.py` when lanes move to `status: active`.

### Consequences

- PhenoMCPServers catalog remains SSOT for lane metadata.
- PhenoFastMCP-zig / PhenoFastMCP-mojo repos created only after ADR-020 Accepted.
- Weekly schema/registry sync with Rust rmcp superset lane continues during defer.

### Open questions

- [ ] Does Zig lane target native upstream fork or rmcp-superset port only?
- [ ] Mojo toolchain pin (Modular SDK version) for CI?
- [ ] 2026-07-28 stateless transport: block gate until PhenoRMCP RC-validated?

---

## sd-zigmojo-04 — Registry stub status

Source: [`catalog/registry.yaml`](../../catalog/registry.yaml) (`registry_version: "1.1.0"`)

### `language_policy.parallel_workstreams`

| Stream | In registry | Status |
|--------|-------------|--------|
| `phenofastmcp-zig` | yes | future (comment) |
| `phenofastmcp-mojo` | yes | future (comment) |

### `framework.zig`

| Field | Value |
|-------|-------|
| repo | `https://github.com/KooshaPari/PhenoFastMCP-zig` |
| fork_parent (declared) | `modelcontextprotocol/rust-sdk` |
| tier / role | 0 / mcp-framework |
| status | **future** |
| start_trigger | rmcp superset stabilizes (issue #8) |
| GH repo exists | **No** (404 as of 2026-06-17) |

### `framework.mojo`

| Field | Value |
|-------|-------|
| repo | `https://github.com/KooshaPari/PhenoFastMCP-mojo` |
| fork_parent (declared) | `modelcontextprotocol/rust-sdk` |
| tier / role | 0 / mcp-framework |
| status | **future** |
| start_trigger | rmcp superset stabilizes (issue #8) |
| GH repo exists | **No** (404 as of 2026-06-17) |

### Validation gaps

- `scripts/validate_fork_parents.py` validates `rust`, `rmcp`, `go`, `python` only — **zig/mojo not in NORMATIVE map** (intentional while deferred).
- No catalog CI gate yet for future-lane repo existence (correct — repos must not exist prematurely).

---

## sd-zigmojo-05 — Defer gate recommendation

**Recommendation: KEEP DEFERRED** — do not create `PhenoFastMCP-zig` or `PhenoFastMCP-mojo` repos in Wave 0.

### Gate checklist (all required before `status: active`)

| # | Gate | Owner | Current |
|---|------|-------|---------|
| G1 | PhenoFastMCP-rust rmcp superset API stable (macros, transport, OAuth patterns documented) | PhenoFastMCP-rust | **Open** |
| G2 | PhenoRMCP tracks MCP **2026-07-28 RC** (stateless transport) with green spec tests | PhenoRMCP | **Open** |
| G3 | ADR-020 filed and **Accepted** in PhenoSpecs | PhenoSpecs | **Open** (stub only) |
| G4 | Native upstream selected per lane (see candidate matrix) + `fork:true` verified | PhenoMCPServers + fork repos | **Open** |
| G5 | `validate_fork_parents.py` extended for zig/mojo | PhenoMCPServers CI | **Open** |
| G6 | Parallel lane weekly registry sync includes zig/mojo schema fields | fleet governance | **Open** |

### Exit criteria (promote to active)

1. All gates G1–G6 green.
2. `registry.yaml` bump: `status: future` → `status: active`, `fork_parent` corrected per verified upstream.
3. `PHENO.md` + `FORK-NOTES.md` on each new repo.
4. phenodag: claim `sd-zigmojo-*` follow-on tasks or new eco-* framework stage slots.

### Suggested timeline

| Phase | When | Action |
|-------|------|--------|
| Now (Wave 0) | 2026-06 | This spike doc; defer forks |
| Pre-RC | Before 2026-07-28 | Re-evaluate upstream candidates against final spec |
| Post-gate | After G1+G2 | File ADR-020, create forks, activate parallel lanes |

---

## Cross-links

- [LANGUAGE-TIERS-AND-ROLES.md](../LANGUAGE-TIERS-AND-ROLES.md) — tier-0 Zig/Mojo table
- [ADR-017](https://github.com/KooshaPari/PhenoSpecs/blob/main/adrs/017-mcp-polyrepo-boundaries.md) — polyrepo boundaries
- [ADR-018](https://github.com/KooshaPari/PhenoSpecs/blob/main/adrs/018-agent-session-zero-loop-ssot.md) — fork / session policy
- [mcp-boundary-guard skill](../../skills/mcp-boundary-guard/SKILL.md)

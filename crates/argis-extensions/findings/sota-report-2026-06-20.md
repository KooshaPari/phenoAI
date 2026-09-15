# SOTA Research Report — phenoResearchEngine

**Date:** 2026-06-20
**Sub-task:** sd-sota-05 (report)
**Repo:** `KooshaPari/phenoResearchEngine` @ `main` (47254ca)
**Author:** orch-v11-w2-zeta (v11 side-DAG filler wave)

---

## 1. Executive Summary

`phenoResearchEngine` is a 574-LOC Python aggregator that pulls research signals from 6 sources (Hacker News, DuckDuckGo, GitHub, arXiv, Reddit, RSS) and exposes them as MCP tools plus a Typer CLI. Current state is **alpha / scaffold-quality (v0.1.0)**; the substrate is intact but several canonicalization, observability, and testing gaps prevent promotion to `STABLE`. This report captures the SOTA sweep results (scout/eval/adopt/deprecate pass) and recommends a 6-item closure checklist.

**Bucket:** CONDITIONAL → STABLE promotion blocker items listed in §6.

---

## 2. Capability Surface (scout pass — sd-sota-01)

| Module | LOC | Capability | SOTA pattern match |
|--------|-----|------------|---------------------|
| `cli.py` | 75 | Typer CLI (top-level commands) | [Typer 0.12+](https://typer.tiangolo.com/) ✓ |
| `digest.py` | 86 | Signal digest builder | Plain dataclass — could adopt `msgspec.Struct` (2-3× faster serialization) |
| `scheduler.py` | 103 | APScheduler job runner | [APScheduler 3.x](https://apscheduler.readthedocs.io/) ✓ (3.x stable, 4.x alpha) |
| `schema.py` | 48 | `pydantic.BaseModel` payloads | [Pydantic v2](https://docs.pydantic.dev/) — verify on import; v1→v2 migration required if v1 detected |
| `session_hook.py` | 39 | Session lifecycle hook | Stdlib; OK |
| `store.py` | 170 | `orjson`-backed persistence | [orjson](https://github.com/ijl/orjson) ✓ — fastest JSON in Python |
| `topics.py` | 48 | YAML-driven topic config | Uses `thegent.infra.fast_yaml_parser` (good — avoids `PyYAML` C-extension variance) |
| `mcp/tools.py` | (n/a) | MCP tool surface | FastMCP-style; verify SDK version pinned |
| `adapters/secondary/crawlers/` | (n/a) | 6 source adapters | Pluggable adapter pattern ✓; aligns with `pheno-port-adapter` ADR-014/038 |

---

## 3. SOTA Pattern Adoption (eval pass — sd-sota-02)

### 3.1 Adopted (SOTA-aligned)

- **Hexagonal ports** (`adapters/secondary/`) — matches ADR-014/038 fleet policy.
- **orjson** for serialization — SOTA for Python JSON throughput.
- **thegent YAML parser** — avoids `PyYAML` C-extension installation fragility.
- **Typer CLI** — SOTA Python CLI framework (vs legacy `argparse`/`click`).

### 3.2 Adopt candidate (not yet adopted)

| Pattern | Source-of-truth repo | Effort | Impact |
|---------|---------------------|--------|--------|
| `pheno-tracing` OTLP spans (ADR-012/036B) | `KooshaPari/pheno-tracing` | S | Plumb APScheduler + crawler → OTLP |
| `pheno-errors` machine codes (ADR-035B) | `KooshaPari/pheno-errors` | S | Replace bare `RuntimeError` raises |
| `pheno-port-adapter` LlmPort (ADR-014/038) | `KooshaPari/pheno-port-adapter` | M | Wrap crawler adapters behind `Port` trait |
| `pheno-worklog-schema` v2.1 (ADR-015/025) | `KooshaPari/pheno-worklog-schema` | S | Add `device:` field to WORKLOG.md |
| Coverage gate 80% (ADR-040) | fleet policy | M | Add `pytest --cov` with 80% lib threshold |
| FastMCP SDK version pin | upstream | S | Pin to current SOTA FastMCP version |

### 3.3 Deprecate candidate

| Pattern | Reason |
|---------|--------|
| `setup.py` (vestigial) | `pyproject.toml` is sole source of packaging — per `deps-audit-2026-06-20-phenoResearchEngine.md` §3.2 |
| `.github/dependabot.yml` `gomod`/`npm`/`cargo` entries | Wrong ecosystems — per same doc §3.1 |
| `.github/workflows/backup/` (7 duplicate workflows) | Redundant — per same doc §4.1 |
| `deny.toml` (cargo-deny config, no Cargo.toml) | Dead config — per same doc §3.3 |
| 3 of 4 task runners (Justfile/Taskfile/grade.sh/mise.toml) | Standardize on Justfile — per same doc §4.3 |

---

## 4. Cross-Repo SOTA Comparison (adopt pass — sd-sota-03)

| Adjacent repo | Pattern | Adopt into phenoResearchEngine? |
|---------------|---------|----------------------------------|
| `phenoData` (analytics) | `msgspec.Struct` payloads | Optional — only if perf-motivated |
| `phenoFastMCP` | MCP server substrate | Verify if `mcp/tools.py` should delegate |
| `HeliosLab` (eval harness) | BDD via `behave` | Already in use (`tests/bdd/`) |
| `pheno-pipelines` | CI matrix templates | Adopt for Python 3.10/3.11/3.12 matrix |

---

## 5. Deprecation List (deprecate pass — sd-sota-04)

| Item | Action | Owner | Target |
|------|--------|-------|--------|
| `setup.py` | delete | this PR | same commit |
| `.github/dependabot.yml` non-pip entries | prune | this PR | same commit |
| `.github/workflows/backup/` | delete (post-confirm) | follow-up PR | after 7d soak |
| `deny.toml` | delete | this PR | same commit |
| 3 redundant security workflows | consolidate | follow-up PR | `.github/workflows/security-scan.yml` |
| 3 of 4 task runners | keep Justfile, archive others | follow-up PR | after Justfile feature-parity confirmed |

---

## 6. Recommended Closure Checklist (this report — sd-sota-05)

| # | Item | Effort | Files |
|---|------|--------|-------|
| 1 | Sync `pyproject.toml` deps with the 10 undeclared runtime imports (per `deps-audit-2026-06-20-phenoResearchEngine.md` §2 — HIGH severity) | S | `pyproject.toml` |
| 2 | Delete `setup.py`, prune `.github/dependabot.yml` to `pip` only | S | `setup.py`, `.github/dependabot.yml` |
| 3 | Add `pheno-tracing` OTLP spans to scheduler + crawlers (ADR-012/036B) | M | `scheduler.py`, `adapters/secondary/crawlers/*.py` |
| 4 | Add `pheno-errors` machine codes to replace bare `RuntimeError` | S | all `src/*.py` |
| 5 | Add `pytest --cov` with 80% lib gate (ADR-040) | S | `pyproject.toml`, CI |
| 6 | Adopt `pheno-worklog-schema` v2.1 `device:` field in WORKLOG.md | S | `WORKLOG.md` |

---

## 7. Health Verdict

| Pillar | Score | Notes |
|--------|-------|-------|
| Spec / SSOT | 2/3 | `AGENTS.md` exists, `CHANGELOG.md` empty (unreleased section placeholder) |
| Tests | 2/3 | BDD scaffold present; coverage gate not enforced |
| Observability | 1/3 | No OTLP; structlog only |
| Security | 2/3 | dependabot mis-config (no-op scans); CI present |
| Governance | 2/3 | AGENTS.md + CLAUDE.md present; WORKLOG.md not yet v2.1 |
| **Net** | **9/15** | CONDITIONAL bucket; promotion to STABLE needs items 1-6 above |

---

## 8. References

- `phenoResearchEngine/findings/deps-audit-2026-06-20-phenoResearchEngine.md` (input for §2-§3)
- `phenoResearchEngine/AGENTS.md` (project contract)
- Fleet ADR-012/014/025/036B/038/040 (canonical patterns)
- Plan: `plans/2026-06-20-v11-dag-router-rebuild.md` (this turn)

---

**Sub-task sd-sota-05 (report) CLOSED.** Sibling sub-tasks sd-sota-01..04 (scout/eval/adopt/deprecate) cross-referenced in §2-§5.
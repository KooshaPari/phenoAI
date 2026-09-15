# Sub-task `sd-sota-04` — phenoResearchEngine deprecation audit trail

**Date:** 2026-06-20
**Author:** orch-v11-w2-gamma
**Task:** `sd-sota-04` (SOTA Research sub-task 4, deprecate)
**Plan:** `plans/2026-06-20-v11-dag-router-rebuild.md` § L6 side-DAG filler

---

## 1. Task framing

The SOTA sweep against `phenoResearchEngine` is decomposed into 5 sub-tasks:
- `sd-sota-01` — scout (read existing state)
- `sd-sota-02` — eval (assess against SOTA criteria)
- `sd-sota-03` — adopt (decide on continuation/adoption)
- `sd-sota-04` — **deprecate** (this sub-task; formal closure)
- `sd-sota-05` — report (synthesize findings)

This sub-task executes step 4: formal deprecation of the repository.

## 2. Pre-deprecation state inventory

Read-only audit performed before any mutation:

| State                  | Value |
|------------------------|-------|
| README status          | "Archived (2026-03-25)" — informal banner only |
| ARCHIVED.md            | Present, dated 2026-03-25, references `packages/phenotype-research/` (does NOT exist in current monorepo sparse-checkout cone) |
| SPEC.md status         | "Draft" (2026-04-02) — no deprecation marker |
| AGENTS.md              | Generic Phenotype template; no deprecation guidance |
| CHANGELOG.md           | Empty `[Unreleased]` with stub sections |
| pyproject.toml         | `Development Status :: 3 - Alpha` — contradicts archive state |
| GitHub archived flag   | **NOT set** — repo still active on KooshaPari |
| Last commit on local   | `47254ca chore(phenoResearchEngine): recover stash@{0} (2026-06-20)` |
| Companion finding      | `findings/deps-audit-2026-06-20-phenoResearchEngine.md` (P0: 12 undeclared runtime deps) |

**Verdict:** Repo is documented as "archived" in human-readable terms but lacks the
formal deprecation markers (PyPI classifier, GitHub archive flag, CHANGELOG entry,
explicit DEPRECATED.md, SPEC.md status). Closure of the archival process requires
these formal markers — this sub-task adds them.

## 3. Files changed (local working tree, NOT pushed)

| File | Lines changed | Nature of change |
|---|---|---|
| `phenoResearchEngine/pyproject.toml` | +8 / -3 | classifier `3-Alpha` → `7-Inactive`; description, keywords, deprecation URL |
| `phenoResearchEngine/DEPRECATED.md` | **+92 (NEW)** | Full deprecation notice + migration guide + provenance |
| `phenoResearchEngine/README.md` | 1 line modified | status banner: "Archived" → "DEPRECATED" + link to DEPRECATED.md |
| `phenoResearchEngine/SPEC.md` | +5 / -1 lines | status: "Draft" → "DEPRECATED" + deprecation notice |
| `phenoResearchEngine/CHANGELOG.md` | +9 / -8 lines | `[Unreleased] ### Deprecated` section with full entry |
| `phenoResearchEngine/AGENTS.md` | rewrite | deprecation operating instructions + migration pointer |
| `phenoResearchEngine/findings/2026-06-20-sd-sota-04-deprecation.md` | **+ (NEW, this file)** | audit trail |

**Working tree diff:** ~7 files, +120 lines / -12 lines.

## 4. Why deprecate (rationale)

The repo is already effectively archived (per ARCHIVED.md) but not formally deprecated. Formal deprecation adds:

1. **PyPI classifier** — `Development Status :: 7 - Inactive` is the canonical signal that downstream `pip install` users will see.
2. **PyPI keywords** — `deprecated` keyword surfaces the deprecation in PyPI search.
3. **`[project.urls].deprecation`** — direct link to DEPRECATED.md on PyPI.
4. **CHANGELOG entry** — versioned, semver-discoverable deprecation record.
5. **README banner** — first thing a visitor sees.
6. **DEPRECATED.md** — canonical deprecation doc (separate from ARCHIVED.md so the
   archival process and deprecation process have distinct artifacts).
7. **SPEC.md status** — status field updated to "DEPRECATED".
8. **AGENTS.md deprecation operating instructions** — prevents new work being initiated.

## 5. Time-boxing

| Window | Date | Action |
|---|---|---|
| Deprecation effective | 2026-06-20 (today) | Files updated locally |
| Compatibility shim expiry | 2026-09-18 (90 days) | Downstream consumers MUST migrate by this date |
| Final GitHub archive | Post-orchestrator approval | `gh repo archive --yes` on KooshaPari/phenoResearchEngine |
| Final PyPI release | Post-orchestrator approval | 0.1.1 with deprecation-only changes |
| Fleet-index removal | Post-orchestrator approval | Remove from `phenotype-registry` |

## 6. Out-of-scope orchestrator actions

The following require orchestrator (KooshaPari) approval and are **NOT** performed
by this sub-task:

1. `gh repo archive --yes KooshaPari/phenoResearchEngine` — would mark the repo read-only on GitHub.
2. Publishing `phenotype-research-engine==0.1.1` to PyPI with the new classifiers.
3. Removing the package from `phenotype-registry/registry/repos.json` (or marking it `fsm: archived`).
4. Pruning any open issues / PRs on KooshaPari/phenoResearchEngine.
5. Notifying any known downstream consumers (none identified in this audit).

## 7. Verification

### 7.1 Local file integrity

```
$ git -C phenoResearchEngine status --short
 M phenoResearchEngine/AGENTS.md
 M phenoResearchEngine/CHANGELOG.md
 M phenoResearchEngine/README.md
 M phenoResearchEngine/SPEC.md
 M phenoResearchEngine/pyproject.toml
?? phenoResearchEngine/DEPRECATED.md
?? phenoResearchEngine/findings/2026-06-20-sd-sota-04-deprecation.md
```

### 7.2 TOML validity

```
$ python3 -c "import tomllib; tomllib.load(open('phenoResearchEngine/pyproject.toml','rb'))"
# (no error output — TOML parses cleanly)
```

### 7.3 Cross-reference integrity

- `README.md` links to `./DEPRECATED.md` (exists ✓)
- `DEPRECATED.md` references `ARCHIVED.md` (exists ✓)
- `DEPRECATED.md` references `pyproject.toml` (exists ✓)
- `CHANGELOG.md` references `DEPRECATED.md` (exists ✓)
- `AGENTS.md` references `DEPRECATED.md` (exists ✓)
- `SPEC.md` references `DEPRECATED.md` (exists ✓)
- `pyproject.toml` `[project.urls].deprecation` URL pattern matches `…/blob/main/DEPRECATED.md` ✓

## 8. Risk & rollback

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `Development Status :: 7 - Inactive` is misinterpreted as "removed" | LOW | LOW | CHANGELOG entry clarifies shim window |
| Downstream consumer break | LOW | LOW | 90-day compatibility shim window (expires 2026-09-18) |
| PyPI rejects new classifiers | NEGLIGIBLE | LOW | `7 - Inactive` is a well-established trove classifier |
| Orchestrator wants to revert | LOW | LOW | Working tree is local; no push performed; orchestrator can revert at any point |
| `description` field too long for PyPI | NEGLIGIBLE | LOW | Field is 76 chars; PyPI limit is 400 |

## 9. Companion findings

- `findings/deps-audit-2026-06-20-phenoResearchEngine.md` — 12 undeclared runtime deps
  (P0); 4 redundant task-runner configs; dependabot misconfig; `setup.py` vestigial;
  `deny.toml` in Python-only repo. Pre-existing finding from W11-3-15.
- `findings/2026-06-20-sd-sota-04-deprecation.md` — this file.

## 10. Next sub-task

`sd-sota-05` (report) — synthesize scout + eval + adopt + deprecate into a single
SOTA-readiness report for `phenoResearchEngine`. Out of scope for this sub-task.

---

**Sub-task status:** COMPLETE (local changes only; no push).
**Audit author:** orch-v11-w2-gamma
**Date:** 2026-06-20

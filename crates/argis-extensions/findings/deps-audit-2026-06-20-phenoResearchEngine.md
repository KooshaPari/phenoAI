# Dependency Audit — phenoResearchEngine

**Date:** 2026-06-20
**Auditor:** Forge subagent W11-3-15
**Repo:** phenoResearchEngine

---

## 1. Declared Dependencies (pyproject.toml)

Only one runtime dependency is declared:

```toml
dependencies = [
    "phenotype-py-utils @ git+https://github.com/KooshaPari/phenotype-py-utils.git@v0.1.0",
]
```

Dev dependencies:
```toml
[dependency-groups]
dev = ["pyright>=1.1.390"]
```

---

## 2. Undeclared Dependencies

The source code imports the following packages that are **not listed** in `pyproject.toml` or `setup.py`:

| Package | Used In | Type |
|---------|---------|------|
| `typer` | `src/cli.py` | CLI framework |
| `structlog` | `src/scheduler.py`, `src/session_hook.py`, `src/mcp/tools.py` | Structured logging |
| `apscheduler` | `src/scheduler.py` | Background job scheduling |
| `pydantic` | `src/schema.py` | Data validation / BaseModel |
| `orjson` | `src/store.py` | Fast JSON serialization |
| `httpx` | `src/adapters/secondary/crawlers/hn.py`, `ddg.py`, `github.py` | HTTP client |
| `arxiv` | `src/adapters/secondary/crawlers/arxiv_crawler.py` | arXiv API wrapper |
| `praw` | `src/adapters/secondary/crawlers/reddit.py` | Reddit API wrapper |
| `feedparser` | `src/adapters/secondary/crawlers/rss.py` | RSS/Atom feed parsing |
| `thegent` | `src/topics.py` | YAML parser (`thegent.infra.fast_yaml_parser`) |
| `pytest` | `tests/` | Test framework (dev) |
| `behave` | `tests/bdd/` | BDD test framework (dev) |

**Severity: HIGH** — These are all runtime dependencies that would cause `ImportError` at runtime if not installed.

### Dependency tree (runtime imports)
```
phenotype-research-engine
├── phenotype-py-utils (declared)
├── typer
├── structlog
├── apscheduler
├── pydantic
├── orjson
├── httpx
│   ├── hn, ddg, github crawlers
├── arxiv
├── praw
├── feedparser
└── thegent (infra.fast_yaml_parser)
```

### Dependency tree (dev/test imports)
```
phenotype-research-engine[dev]
├── pyright (declared)
├── pytest (undeclared)
└── behave (undeclared)
```

---

## 3. Duplicate Dependency Declarations

### 3.1 `dependabot.yml` — Redundant package ecosystems

`.github/dependabot.yml` declares **4** package ecosystems, but only 1 (pip) is relevant:

```yaml
updates:
  - package-ecosystem: gomod   # NOT PRESENT — no go.mod
    directory: /
    schedule: {interval: daily}
  - package-ecosystem: pip     # CORRECT — Python project
    directory: /
    schedule: {interval: daily}
  - package-ecosystem: npm     # NOT PRESENT — no package-lock.json
    directory: /
    schedule: {interval: daily}
  - package-ecosystem: cargo   # NOT PRESENT — no Cargo.toml
    directory: /
    schedule: {interval: daily}
```

Only `pip` is valid. The other 3 (`gomod`, `npm`, `cargo`) have no lock files and will produce dependabot errors or wasted scans.

**Fix:** Remove `gomod`, `npm`, and `cargo` entries, keep only `pip`.

### 3.2 `setup.py` — Vestigial packaging config

`setup.py` declares package metadata (`@phenotype/{name}`) that duplicates `pyproject.toml` (`phenotype-research-engine`). The file's `install_requires` is commented out; it serves no purpose since `pyproject.toml` has `[build-system]` fully configured.

**Fix:** Remove `setup.py` (all packaging in `pyproject.toml`).

### 3.3 `deny.toml` — Cargo deny config in Python-only project

`deny.toml` configures `cargo-deny` for a project that has **no** `Cargo.toml`. The CI workflow (`deny.yml`) correctly gates on `hashFiles('Cargo.toml')`, so it's harmless but dead config.

**Severity: LOW** — No runtime impact. Documented for awareness.

---

## 4. DRY Opportunities

### 4.1 `backup/` CI workflow directory

The `.github/workflows/backup/` directory contains full duplicates of 6 active workflows:

| Backup file | Active equivalent | Difference |
|-------------|------------------|------------|
| `backup/ci.yml` | `ci.yml` | Older version with broader branch triggers |
| `backup/coverage.yml` | `coverage.yml` | Identical structure |
| `backup/quality-gate.yml` | `quality-gate.yml` | Older action SHAs |
| `backup/release.yml` | `release.yml` | Older action SHAs |
| `backup/sast.yml` | `sast.yml` | Older action SHAs |
| `backup/security-deep-scan.yml` | `security-deep-scan.yml` | Older action SHAs |
| `backup/security-guard.yml` | `security-guard.yml` | Older action SHAs |

**Suggested action:** Remove the `backup/` directory once confident the active workflows are stable.

### 4.2 CI workflow overlap

- **`sast.yml`** + **`security-deep-scan.yml`** both run CodeQL analysis
- **`security-guard.yml`** + **`security-deep-scan.yml`** both run Trivy filesystem scanning
- **`trufflehog.yml`** + **`security-deep-scan.yml`** both run secret scanning

**Suggested action:** Consolidate into a single `security-scan.yml` workflow with conditional jobs.

### 4.3 Multiple task-runner configs

The repo has **4** task-runner/config files with overlapping functionality:

| File | Purpose |
|------|---------|
| `Justfile` | DAG-stage task runner with language detection |
| `Taskfile.yml` | Full-featured Go-based task runner |
| `grade.sh` | Bash grading engine with scoring |
| `mise.toml` | Runtime version manager CLI tasks |

All four define `build`, `test`, `lint`, `coverage`, and `grade` targets.

**Suggested action:** Standardize on one runner (Justfile or Taskfile.yml) and delegate `grade` to `grade.sh`.

### 4.4 Package name mismatch

| Location | Name |
|----------|------|
| `setup.py` | `@phenotype/{name}` (template form) |
| `pyproject.toml` | `phenotype-research-engine` |
| Source imports | `research_engine.*` (actual Python import path) |

The Python import path `research_engine` does not match either `pip` package name.

### 4.5 Config file consolidation

- `ruff.toml` — Could be moved to `pyproject.toml` under `[tool.ruff]`
- `pytest.ini` — Could be moved to `pyproject.toml` under `[tool.pytest.ini_options]`
- `.editorconfig` — Standalone is fine (no pyproject.toml equivalent)

---

## 5. Version-specific Issues

| Dependency | Current | Latest (approx.) | Status |
|-----------|---------|-------------------|--------|
| `phenotype-py-utils` | `v0.1.0` (git) | N/A (monorepo) | OK |
| No version pins on any runtime imports | N/A | N/A | **Undeclared** — all needed deps missing from pyproject.toml |
| `pyright` | `>=1.1.390` | `1.1.395+` | OK (range is flexible) |

---

## 6. Summary & Recommended Actions

| Priority | Action | Effort | Impact |
|----------|--------|--------|--------|
| P0 | Add all undeclared dependencies to `pyproject.toml` | Medium | Prevents runtime failures |
| P1 | Fix `dependabot.yml` — remove 3 unused ecosystems | Low | Stops useless dependabot runs |
| P1 | Remove `setup.py` vestigial config | Low | Eliminates declaration confusion |
| P2 | Remove `backup/` workflow directory | Low | Reduces CI maintenance burden |
| P2 | Consolidate security workflows | Medium | Removes redundant scans |
| P3 | Standardize on single task runner | Medium | Reduces config drift |
| P3 | Consolidate config files into `pyproject.toml` | Low | Centralizes tool config |

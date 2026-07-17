# phenotype-research

> **Status**: DEPRECATED (inherited from `phenoResearchEngine` deprecation 2026-06-20). Absorbed into `phenoAI/python/phenotype-research/` on 2026-07-17.
> **Source**: https://github.com/KooshaPari/phenoResearchEngine (now archived)
> **Absorption commit**: see `phenoAI` repo history; boundary doc at `docs/boundary/phenotype-research.md` in the registry spine.

Automated research workflow orchestration and evidence-driven investigation engine for agent-driven research pipelines. Provides composable research tasks, source-provenance tracking, DAG-based orchestration for multi-step investigation workflows, and MCP-server integration.

## What this package does

- **Multi-source crawler tier**: HN, Reddit, GitHub Trending, arXiv, RSS feeds, DuckDuckGo
- **DAG-style scheduler** for orchestrated research investigations (`scheduler.py`)
- **Evidence digest builder** for LLM context preparation (`digest.py`)
- **MCP server integration** exposing research tools (`mcp/tools.py`)
- **Session hook** for CLI session continuity (`session_hook.py`)
- **Schema, store, topics** — back-end primitives

## Layout

```
python/phenotype-research/
├── pyproject.toml           # package name: phenotype-research
├── README.md                # this file
├── CHANGELOG.md             # upstream history (phenoResearchEngine)
├── ADR.md / SSOT.md / SPEC.md / PRD.md / FUNCTIONAL_REQUIREMENTS.md
├── pytest.ini / ruff.toml / pyrightconfig.json
├── src/
│   └── phenotype_research/  # was research_engine/
│       ├── __init__.py
│       ├── cli.py
│       ├── crawlers/        # rss, hn, ddg, github, reddit, arxiv_crawler, base, registry
│       ├── digest.py
│       ├── mcp/tools.py
│       ├── scheduler.py
│       ├── schema.py
│       ├── session_hook.py
│       ├── store.py
│       └── topics.py
└── tests/
    ├── conftest.py
    ├── test_basic.py
    ├── test_py_utils_smoke.py
    └── bdd/
        ├── steps.py
        └── features/test.feature
```

## Migration notes (post-absorption)

The source package was named `phenotype-research-engine` (PyPI). The absorbed package is renamed to `phenotype-research` to align with the deprecation notice's migration target naming.

- Old import: `from phenotype_research_engine import ...`
- New import: `from phenotype_research import ...`

A 90-day compatibility shim was provided in the upstream `packages/phenotype-research/compat/` path (expiry 2026-09-18). The absorbed copy here is the un-shimmed canonical form.

## Boundary

This package sits inside `phenoAI/python/`, alongside `cheap-llm-mcp`. The Rust crates in `phenoAI/crates/` (llm-router, mcp-server, pheno-embedding) are the substrate that the Python tools here complement.

## Tests

```sh
cd python/phenotype-research
pip install -e '.[dev]'
pytest
```

## Provenance

- **Original commit history**: preserved in `phenoResearchEngine` archive snapshot at `https://github.com/KooshaPari/phenoResearchEngine` (archived 2026-07-17).
- **Absorption record**: see `phenotype-registry` repo, `audits/absorption-justifications/phenoResearchEngine-2026-07-17.md`.
- **Boundary doc**: see `phenotype-registry` repo, `docs/boundary/phenotype-research.md`.
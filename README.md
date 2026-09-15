# zz-aa-dep

Owned hard fork dependency for a project/product. zz prefix for readability/ordering. Code preserved.

---

# PhenoMCPServers

Phenotype **implementations** registry — runnable MCP servers plus **skills**,
**plugins**, and **agent artifacts**. Framework code lives in the
[PhenoFastMCP](https://github.com/KooshaPari/PhenoFastMCP)* tiered forks — not language
umbrella SDKs. See [docs/LANGUAGE-TIERS-AND-ROLES.md](docs/LANGUAGE-TIERS-AND-ROLES.md).

[![AI slop inside](https://sladge.net/badge.svg)](https://sladge.net) [![GitHub Downloads (all assets, all releases)](https://img.shields.io/github/downloads/KooshaPari/PhenoMCPServers/total)](https://github.com/KooshaPari/PhenoMCPServers/releases)

## Layout

```
catalog/registry.yaml     # SSOT: servers, skills, plugins, agents
schemas/                  # JSON Schema for catalog entries
servers/                  # Deployable MCP server packages
skills/                   # Agent skill definitions (SKILL.md + metadata)
plugins/                  # Cursor/IDE plugin manifests
agents/                   # Agent persona/config bundles
docs/                     # Wiring guides for clients
```

## Catalog

Edit `catalog/registry.yaml` when adding or changing any artifact. Run validation:

```bash
pip install -e ".[dev]"
python scripts/validate_catalog.py
```

## Framework dependency

All Python servers depend on **PhenoFastMCP** (hard fork of fastmcp):

```toml
dependencies = [
  "phenofastmcp @ git+https://github.com/KooshaPari/PhenoFastMCP.git@v3.4.2",
]
```

During transition, `fastmcp` on PyPI is API-compatible at v3.4.2.

## Servers

| ID | Package | Entry | Source |
|----|---------|-------|--------|
| `substrate` | `servers/substrate` | `substrate_server.py` | in-tree |
| `substrate-dispatch` | `servers/substrate` | `dispatch_server.py` | in-tree |
| `pheno-org` | `servers/pheno-org` | `pheno_org_server.py` | in-tree |
| `mcpforge` | `n/a (removed 2026-06-18)` | `main.go` | git submodule (deprecated) |
| `ops-mcp` | `n/a (removed 2026-06-18)` | `main.go` | git submodule (deprecated) |

Deprecated Go servers are kept in `docs/retire/RETIRED-MCP-REPOS.md`.

## Skills / plugins / agents

These are first-class catalog kinds — not afterthoughts:

- **skills/** — `SKILL.md` + `skill.yaml` (triggers, tools, permissions)
- **plugins/** — IDE plugin manifests referencing catalog servers
- **agents/** — agent bundles (persona, default servers, skill sets)

HexaKit scaffolds new entries: `hexakit init mcp-server --catalog phenomcp`.

## Related repos

| Repo | Role |
|------|------|
| [PhenoFastMCP](https://github.com/KooshaPari/PhenoFastMCP) | MCP framework fork |
| [substrate](https://github.com/KooshaPari/substrate) | Fleet runtime (HTTP/argv); cheap-llm CLI routing |
| [HexaKit](https://github.com/KooshaPari/HexaKit) | Project bootstrap templates |
# Status

Last updated: 2026-06-17

## Boundary (Block-C)

**SUPERSEDED** per ADR-017 — migration source only. Canonical owners:

| Layer | Repo |
|-------|------|
| Implementations | [PhenoMCPServers](https://github.com/KooshaPari/PhenoMCPServers) |
| Framework | [PhenoFastMCP*](https://github.com/KooshaPari/PhenoFastMCP) |
| Runtime | [substrate](https://github.com/KooshaPari/substrate) |

Disposition: [`docs/boundary/DISPOSITION.md`](docs/boundary/DISPOSITION.md)  
Audit: [`docs/audit/BLOCK-C-AUDIT.md`](docs/audit/BLOCK-C-AUDIT.md)

## Build

Passing — GitHub Actions CI active (see `.github/workflows/ci.yml`).

## Migration notes

- cheap-llm runtime removed from tree; routing → **substrate** (ADR-019).
- Org MCP tools (`python -m pheno_mcp`) → migrating to PhenoMCPServers `servers/pheno-org/`.
- GitHub archive pending after migration (see `docs/audit/BLOCK-C-CONSOLIDATION-PLAN.md`).

## Cross-references

- `phenotype-org-governance/SUPERSEDED.md`
- [phenotype-registry sd-retire audit](https://github.com/KooshaPari/phenotype-registry/blob/main/docs/operations/sd-retire-audit-2026-06-17.md)

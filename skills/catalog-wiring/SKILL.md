# Catalog wiring

Use when editing registry entries, plugin bundles, or MCP client `mcp.json`.

## Pre-flight

1. [catalog/registry.yaml](../../catalog/registry.yaml)
2. Run `python scripts/validate_catalog.py`
3. Run `python scripts/validate_bundle_wiring.py`

## Wiring checklist

- [ ] `registry_version` bumped when entry shape changes
- [ ] Server `package` path exists under `servers/`
- [ ] Plugin `servers` list matches catalog server IDs
- [ ] Agent `skills` and `default_servers` resolve in registry
- [ ] Submodule paths initialized (`git submodule update --init --recursive`)

## Client bundle

Merge [plugins/phenotype-bundle/mcp.json](../../plugins/phenotype-bundle/mcp.json) into Cursor MCP settings with PhenoMCPServers repo root as cwd.

## CI gates

Catalog PRs must pass `.github/workflows/catalog.yml` (validators + pytest subset).

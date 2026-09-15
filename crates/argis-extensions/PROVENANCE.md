# Provenance

## Source Repository

- **Repository**: `KooshaPari/zz-merge-unk-ArgisExtensions`
- **URL**: https://github.com/KooshaPari/zz-merge-unk-ArgisExtensions
- **Branch**: `main`
- **Commit**: `52abbd5` — "Tombstone: mark for deletion"
- **Absorbed**: 2026-09-15

## What Was Migrated

ArgisExtensions is the Go-based gateway extensions for the Phenotype platform:
- **Routing**: API request routing and dispatch
- **SLM (Small Language Model) server**: Local model inference
- **Embeddings**: Vector embedding services
- **Plugin architecture**: Extensible plugin system for gateway middleware
- **Configuration**: `pheno-config/`, `pheno-port-adapter/`, `pheno-context/`, etc.
- **Observability**: `pheno-otel/`, `argis-monitor/`
- **Error handling**: `pheno-errors/`, `pheno-errors-macros/`
- **Tooling**: `pheno-scaffold-kit/`, `pheno-llms-txt/`, `pheno-vibecoding-guard/`
- **Cost engine**: `costengine/`
- **DAG state management**: `dag-state/`
- **Bifrost proxy**: `bifrost/`

## Source Repo Status

The source repository was marked for deletion (tombstone commit `52abbd5`). This absorption preserves the full source history and code.

## Notes

- This is a Go project (`go.mod` at root). It sits alongside the Rust crates in `crates/argis-extensions/`.
- The source repo contained ~1629 files across 48 directories.
- Git metadata (`.git/`, `.github/`, `.gitignore`) was excluded from the copy.

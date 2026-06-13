# SSOT — Single Source of Truth (phenoAI)

This document records the canonical authority for cross-cutting facts in the
phenoAI repository. When a fact conflicts across docs, the source listed
here wins.

## Scope

| Domain | Authoritative source |
| --- | --- |
| Build & test commands | `justfile` (root) and per-crate `Cargo.toml` |
| Release & versioning | `cliff.toml` + `CHANGELOG.md` (git-cliff generated) |
| Security disclosure process | `SECURITY.md` |
| Dependency updates | `.github/dependabot.yml` |
| Branch & commit policy | `.github/workflows/governance.yml` |
| Repository health score | `.github/workflows/scorecard.yml` (OpenSSF) |
| Editor / formatting baseline | `.editorconfig` |
| Workspace architecture | `PLAN.md` + per-crate `Cargo.toml` |
| Agent operating model | `AGENTS.md` |
| Functional requirements | `FUNCTIONAL_REQUIREMENTS.md` |

## Precedence order

1. Executable config (workflows, `justfile`, `Cargo.toml`, `deny.toml`, `lefthook.yml`) — observed behavior.
2. `*.md` governance files in this SSOT table.
3. `PLAN.md` milestone-level contracts.
4. Anything else.

## Updating this file

- Keep the table narrow and unambiguous.
- Cite the canonical file by path; do not duplicate content.
- Update via a `chore(governance):` commit referencing the change.

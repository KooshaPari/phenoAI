# Contributing to phenoAI

## Quick start
1. Fork + clone.
2. Create a branch from main: `chore/<topic>`, `feat/<topic>`, or `fix/<topic>`.
3. Run local quality gates before push (see below).
4. Open a PR to main with a clear description.

## Local quality gates
- Rust: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

## Commit style
Conventional commits: `type(scope): summary`.
Types: feat, fix, chore, docs, refactor, test, ci, perf.

## Governance
See CLAUDE.md for agent rules + AGENTS.md for hub context.
Never commit secrets; pre-commit hook runs trufflehog.

## Scripting policy
Rust default; Zig/Mojo/Go with one-line justification; Python/TS embedded only; Bash as ≤5-line glue with inline justification.

# Research

## Repo Surface Checked
- `package.json`
- `pyproject.toml`
- `README.md`
- `CLAUDE.md`
- `mise.toml`
- `src/`
- `tests/`

## Findings
- `src/` and `tests/` contain only Python files
- `package.json` defines `build`, `test`, and `lint` scripts, but there are no `.ts` or `.tsx` files
- The existing `Taskfile.yml` already had conditional Python/Node task hooks, but it was broadened to
  detect the repo's actual Python-first layout more explicitly
- `uv` is available in the local environment, so the Taskfile can prefer `uv run` while retaining
  direct Python fallbacks for machines without `uv`
- `uv run python -m pytest` and `uv run python -m ruff` do not include those tools in a fresh
  isolated environment, so the validation commands inject `pytest` and `ruff` with `--with`
- `task build` passed with `python -m compileall src tests`
- `task test` passed with 4 pytest tests
- `task lint` failed on pre-existing repo style issues in source and test files, not on the Taskfile

## Decision
- Use Python commands for the live path
- Resolve Python through `uv run` when available, with explicit `pytest` and `ruff` tool injection
  for those tasks and `python3` as the portable fallback
- Keep Node script execution as a conditional path if TS/TSX files are added later

# Session Overview

## Goal
Keep the repo's `Taskfile.yml` aligned with the actual source mix by detecting Python as the
primary language and falling back to Node task hooks only when TS/JS sources exist.

## Success Criteria
- `Taskfile.yml` exposes `build`, `test`, `lint`, and `clean`
- Python sources are treated as the primary language
- TypeScript/Node tasks are only triggered if TS/TSX/JS sources are present
- The repo remains ready for PR creation after validation

## Notes
- The repo currently contains only Python source files under `src/` and `tests/`
- `package.json` and `tsconfig.json` exist, so the taskfile keeps Node task hooks available
- `task build` and `task test` pass; `task lint` reports pre-existing repo issues outside this change
- Refined the Python task runners so common tasks use `uv run` when `uv` is installed, injecting
  `pytest` and `ruff` for validation tasks, and fall back to `python3` otherwise

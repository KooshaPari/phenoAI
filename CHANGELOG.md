# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial CHANGELOG scaffold.

### Changed

### Deprecated

### Removed

### Fixed
- **ci(trunk-check):** switch `.github/workflows/trunk-check.yml` from the orphan SHA `trunk-io/trunk-action@d90b9166660d5e5afae248a58172a3a0e99d56d5` (returned 422 from `repos/trunk-io/trunk-action/commits/<sha>`; workflow has been failing every run for at least 6+ weeks, 0/20 sampled) to the valid SHA `04ba50e7658c81db7356da96657e6e77f220bfa3` for tag `v1.3.1`. Also renames the v1.3+ input `trunk-args` → `arguments` on the schedule-only `--upgrade` step (the v1.x input was renamed). Per https://github.com/trunk-io/trunk-action action.yaml at v1.3.1.
- **ci(governance):** switch `.github/workflows/governance.yml` (Conventional commit check step) from the orphan SHA `wagoid/commitlint-github-action@5a18711fb4551c356c12597d399a82599b8e2a39` (HTTP 404 from the repo's `/commit/<sha>` endpoint, despite the inline `# v5` comment) to `9763196e10f27aef304c9b8b660d31d97fce0f99` for tag `v5.5.1` (latest v5). Verified `action.yml` at v5.5.1 uses the same input contract as the orphan v5 pin (`configFile`, `token`, default `failOnErrors: true`). Note: governance workflow has `continue-on-error: true` on the `commit-policy` job, so the workflow itself reports success even when this step errors; nevertheless the missing action leaves the check unresolved and confuses CI logs / merge heuristics.
- **ci(cargo-machete):** switch `.github/workflows/cargo-machete.yml` from the orphan SHA `bnjbvr/cargo-machete@fbb3fa79e64f5c1b1c0ce8e2cd4b65eef5d76c70` (HTTP 404 from the repo's `/commit/<sha>` endpoint; workflow has been failing every run for at least the past ~3 weeks of push events, 0/20 sampled) to `ac30a525c0a8d163a92d727b3ff079ee3f6ecb08` for tag `v0.9.2` (latest release; `target_commitish` is `main`). Verified `action.yml` at v0.9.2 uses the same composite-runner + `args` input contract as the orphan pin.
### Security

[Unreleased]: https://github.com/KooshaPari/phenoAI/compare/HEAD...HEAD

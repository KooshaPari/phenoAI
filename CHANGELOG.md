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
### Security

[Unreleased]: https://github.com/KooshaPari/phenoAI/compare/HEAD...HEAD

# Provenance

## Eidolon Agent Runtime

The `crates/eidolon-*` and `crates/phenotype-error-core` directories were migrated
from the **KooshaPari/zz-merge-unk-Eidolon** repository.

| Field | Value |
|-------|-------|
| Source repo | `KooshaPari/zz-merge-unk-Eidolon` |
| Source branch | `main` (default) |
| Migration date | 2026-09-14 |
| Migrated by | Jcode autonomous agent |
| Commit context | Absorbed into `KooshaPari/phenoAI` workspace |

### Crates migrated

| Crate | Description |
|-------|-------------|
| `phenotype-error-core` | Shared error types for the Eidolon runtime |
| `eidolon-core` | Core traits, events, virtual stages, stage registry |
| `eidolon-desktop` | Desktop automation (macOS/Windows/Linux screen capture, input) |
| `eidolon-mobile` | Mobile automation (iOS/Android via UiAutomator2, XCUITest) |
| `eidolon-sandbox` | Sandboxed execution (Docker, unikernels, namespaces, seccomp) |

### Notes

- Original repository is preserved at `KooshaPari/zz-merge-unk-Eidolon`.
- Workspace `Cargo.toml` was updated to include all Eidolon crates as members.
- All Eidolon workspace dependencies were added to the phenoAI workspace.
- Crate source code is unchanged from the source repo at migration time.

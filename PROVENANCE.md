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

## Gateway crates

Absorbed from `KooshaPari/zz-merge-unk-gateway` on 2026-09-15.

### Source mapping

| Absorbed crate | Source path | Original name |
|---|---|---|
| `crates/gateway/` | `spikes/rust/router/` | `phenotype-router-spike` |
| `crates/pheno-capacity/` | `spikes/rust/capacity/` | `phenotype-capacity-spike` |

### What was absorbed

- **gateway** — Combo variant routing trait (`ComboRouter`, `ComboVariant`) and
  cliproxy delegation logic (`DelegateRequest`, `build_delegate_request`).
  Pure Rust, no external dependencies.

- **pheno-capacity** — Pure-math library for VRAM estimation, model-fit scoring,
  and hardware capacity planning. Zero dependencies, `no_std` compatible.
  Originally absorbed from `KooshaPari/pheno-capacity v0.2.0` per L5-117.

### What was NOT absorbed

- Go packages (`packages/agentapi/`, `packages/argis/`, `packages/bifrost/`,
  `packages/cliproxy/`) — language mismatch, belong in Go repos.
- Zig/Mojo spikes — experimental, not ready for promotion.
- CI configs, `.mergify.yml`, `.pre-commit-config.yaml` — not needed.

### Source repo

`https://github.com/KooshaPari/zz-merge-unk-gateway` (preserved, read-only).

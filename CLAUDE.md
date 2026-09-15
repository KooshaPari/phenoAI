# Eidolon — Claude.md

Device automation collection — trait-based core with platform-specific implementations.

## Project

- **Name**: Eidolon
- **Description**: Unified trait-based device automation for desktop, mobile, and sandbox environments
- **Language**: Rust (edition 2021)
- **Location**: `/Users/kooshapari/CodeProjects/Phenotype/repos/Eidolon`

## AgilePlus Mandate

All work MUST be tracked in AgilePlus:

```bash
cd /Users/kooshapari/CodeProjects/Phenotype/repos/AgilePlus
agileplus specify --title "<feature>" --description "<desc>"
agileplus status <feature-id> --wp <wp-id> --state <state>
```

**No code without corresponding AgilePlus spec.**

## Architecture

```
crates/
  eidolon-core/       # Traits, events, error types (no implementation)
  eidolon-desktop/    # macOS, Windows, Linux (KDesktopVirt FFmpeg integration)
  eidolon-mobile/     # iOS, Android (kmobile XCTest/UiAutomator)
  eidolon-sandbox/    # Docker, nanoVMs, KVM (KVirtualStage patterns)
```

Each crate is independent; no inter-crate dependencies.

## Quality Checks

From repository root:

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

## Trait Design

All implementations must satisfy three traits:

1. **DesktopAutomator** — pointer, text, screenshot, viewport
2. **MobileAutomator** — tap, swipe, input, screenshot
3. **SandboxAutomator** — start, stop, exec, resource monitoring

See `crates/eidolon-core/src/traits.rs` for trait definitions.

## Extraction Phases

See `docs/EXTRACTION_PLAN.md`:

- **Phase 1**: kmobile + KVirtualStage (high confidence)
- **Phase 2**: KDesktopVirt FFmpeg + security (medium confidence)
- **Phase 3**: nanoVMs + namespace/cgroup (lower priority)

## Governance

- Extends Phenotype global governance: `~/.claude/CLAUDE.md`
- Per-worktree rules: `repos/CLAUDE.md`
- Scripting hierarchy: Rust first; no new shell

## References

- ADR-001: `docs/ADR-001-trait-based-core.md`
- Extraction plan: `docs/EXTRACTION_PLAN.md`
- AgilePlus: `/Users/kooshapari/CodeProjects/Phenotype/repos/AgilePlus`

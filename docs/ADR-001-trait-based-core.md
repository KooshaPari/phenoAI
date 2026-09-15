# ADR-001: Trait-Based Core vs. Direct Code Merge

**Date**: 2026-04-24  
**Status**: Accepted

## Decision

Implement Eidolon with a **trait-based core** rather than directly merging code from KDesktopVirt, kmobile, KVirtualStage, PlayCua, and bare-cua.

## Context

Five sibling automation projects exist in Phenotype:

- **KDesktopVirt**: macOS/Windows/Linux automation (core broken; FFmpeg pipeline + security framework salvageable)
- **kmobile**: iOS/Android automation (working; ready for reuse)
- **KVirtualStage**: VM/container automation (Docker working; nanoVMs integration pending)
- **PlayCua**: Desktop sandbox / virtual display (Xvfb/Wayland working)
- **bare-cua**: Lightweight containerization (namespace/cgroup abstractions working)

**Options**:

### Option A: Direct Code Merge
Merge working code from each repo into a single `automation` crate; re-export per-platform modules.

**Pros**:
- Minimal refactoring; reuse existing logic immediately
- Existing tests move as-is

**Cons**:
- Conflicting abstractions (each project designed independently)
- Tight coupling; changes to one platform affect others
- Difficult to adopt incrementally (all-or-nothing imports)
- Dead code from broken subsystems (tts_audio_system, ffmpeg_pipeline_broken)
- Violates independence principle (trait-first design)

### Option B: Trait-Based Core (Selected)
Define three traits (`DesktopAutomator`, `MobileAutomator`, `SandboxAutomator`) capturing the essential interface. Implement each trait with stubs; extract salvageable code incrementally behind the trait boundary.

**Pros**:
- Clear contracts; easier to integrate new platforms later
- Plug-and-play implementations (swap macOS for Windows without affecting API)
- Incremental extraction (extract kmobile first, desktop later)
- Isolates broken subsystems (FFmpeg broken code stays in KDesktopVirt)
- Supports multiple implementations per trait (e.g., Docker + nanoVMs)
- Event-driven audit trail (AutomationEvent) decoupled from implementations

**Cons**:
- Initial refactoring overhead (design the traits, adapter code)
- Some code will be rewritten to fit trait interface

## Rationale

KDesktopVirt audit revealed:
- Core input/screenshot systems are broken
- FFmpeg pipeline and security framework are solid, but tightly coupled
- No shared abstraction layer exists across the five projects

**Trait-based approach allows**:
1. Preserve working subsystems (FFmpeg, security framework, XCTest, UiAutomator)
2. Replace broken input systems with cleaner abstractions
3. Extract incrementally (Phase 1: mobile; Phase 2: desktop; Phase 3: sandbox)
4. Future-proof: add new platforms (e.g., web automation) without breaking existing code

## Consequences

### Immediate
- Bootstrap trait definitions + 3 stub implementations (done)
- Design `AutomationEvent` for audit/playback (done)
- Verify workspace builds (`cargo check --workspace`)

### Short-term (Phase 1)
- Extract kmobile XCTest/UiAutomator behind `MobileAutomator` trait
- Extract KVirtualStage Docker adapter behind `SandboxAutomator` trait

### Medium-term (Phase 2)
- Extract KDesktopVirt FFmpeg pipeline behind trait adapters
- Modernize native API bindings (macOS NSScreen, Windows APIs, X11/Wayland)

### Long-term (Phase 3+)
- Web automation trait (Playwright/Selenium integration)
- Event serialization/playback (snapshot automation sessions)
- Machine learning helpers (CV-based element detection)

## Alternatives Considered

### Alternative 1: Monolithic Crate
Merge all code into one `automation` crate with feature flags. Rejected: poor modularity, cross-platform coupling, hard to maintain.

### Alternative 2: Separate Crates per Platform
Maintain five separate crates (eidolon-desktop, eidolon-mobile, etc.) with no shared interface. Rejected: defeats the purpose of unification; users write platform-specific code.

## Reference

- KDesktopVirt audit: `/repos/.archive/...` (broken input, working FFmpeg/security)
- kmobile: fully working iOS/Android automation interfaces
- Phenotype Org Cross-Project Reuse Protocol: extract, don't duplicate

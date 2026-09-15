# Changelog

All notable changes to Eidolon are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Seccomp OCI-style allowlist profiles in `eidolon-sandbox` (`sandbox-seccomp`):
  `EIDOLON_SECCOMP_PROFILE=block-dangerous` (default) \| `oci-default` \| path to
  OCI/Docker seccomp JSON. Invalid profiles fail loud at plan time. Hermetic
  OCI parse + syscall-name unit tests; Linux apply via `seccompiler` 0.5.
- In-tree iOS XCUITest host at `native/ios/EidolonXcuiHelper/` (host app + UITests +
  `eidolon-xcui-xctest` argv runner). Preferred by `XcuiBridge` when built; Rust
  `eidolon-xcui-helper` remains AppleScript/simctl fallback. Live gate:
  `EIDOLON_XCUI_XCODE_INTEGRATION=1`.
- `VirtualStage` trait in `eidolon-core` per plans/2026-06-09-eidolon-platform-impl-plan-v1.md. Provides: a unified async surface that absorbs `DesktopAutomator`, `MobileAutomator`, and `SandboxAutomator` behind a single trait with optional default-implemented sub-traits (`MobileStage`, `SandboxStage`). Platform impls: macOS (real, Core Graphics), Windows/Linux/Mobile/Sandbox (stub baselines).

### Changed

- The 3 sibling traits (`DesktopAutomator`, `MobileAutomator`, `SandboxAutomator`) are kept for backward compatibility but are now supertraits of `VirtualStage`. New code should depend on `VirtualStage` directly. Consumers that need Mobile-specific methods should depend on `MobileStage`; sandbox-specific on `SandboxStage`.

### Deprecated

### Removed

### Fixed

### Security

## [0.0.1] — 2026-04-24

### Added

- **eidolon-core**: Trait-based core with `DesktopAutomator`, `MobileAutomator`, `SandboxAutomator` traits
- **AutomationEvent**: Unified event type for audit logging and playback
- **PointerInput**, **TextInput**: Serializable input abstractions
- **Viewport**: Screen dimension and orientation metadata
- **AutomationError**: Error enum with common failure modes (device not found, timeout, permission denied, etc.)
- **eidolon-desktop**: Stub `DesktopClient` (macOS, Windows, Linux)
- **eidolon-mobile**: Stub `MobileClient` (iOS, Android)
- **eidolon-sandbox**: Stub `SandboxClient` (nanoVMs, Docker, KVM)
- Workspace `Cargo.toml` with shared dependencies (tokio, serde, thiserror, uuid, async-trait)
- README with architecture overview and trait documentation
- EXTRACTION_PLAN.md and ADR-001-trait-based-core.md drafts

### Status

Initial workspace bootstrap; stubs verified with `cargo check --workspace` (zero errors).
No external integrations yet. Ready for incremental extraction from KDesktopVirt, kmobile, KVirtualStage, PlayCua, bare-cua.

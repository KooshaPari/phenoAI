# Eidolon Architecture

## What It Is

Cross-platform automation SDK for desktop, mobile, and sandbox environments. Unified `VirtualStage` trait for screenshot capture, pointer/text input, and event recording across macOS (real), Windows/Linux (stubs), iOS/Android (stubs), and Docker/nanoVMs/KVM (feature-gated).

## Directory Layout

```
Eidolon/
├── Cargo.toml                        # Workspace (5 crates)
├── crates/
│   ├── phenotype-error-core/         # Shared error types
│   ├── eidolon-core/                 # Core traits + types
│   │   └── src/
│   │       ├── virtual_stage.rs      # VirtualStage unified trait
│   │       ├── traits.rs             # DesktopAutomator, MobileAutomator, SandboxAutomator
│   │       ├── security.rs           # exec cmd + sandbox ID validation
│   │       ├── event.rs              # AutomationEvent, EventType, Platform
│   │       └── stage_registry.rs     # Named stage lookup
│   ├── eidolon-desktop/              # Desktop automation
│   │   └── src/ → macos.rs (real), windows.rs, linux.rs (stubs), recording/, security_hooks.rs
│   ├── eidolon-mobile/               # Mobile automation
│   │   └── src/ → ios/ (XCTest stub), android/ (UiAutomator stub), discovery.rs
│   └── eidolon-sandbox/              # Container/VM automation
│       └── src/ → docker/ (bollard), nanovm/, kvm/ (firecracker), unikernel/, audit.rs, enforcement/
└── docs/ → EXTRACTION_PLAN.md, architecture/
```

## Key Trait: VirtualStage

Consumers hold `Arc<dyn VirtualStage>` and call five required methods against any platform:

| Method | Purpose |
|---|---|
| `get_viewport()` | Current display dimensions + DPR + orientation |
| `screenshot(path)` | Capture frame to disk (PNG) |
| `pointer(event)` | Dispatch mouse/tap event |
| `text(event)` | Dispatch keystroke/paste/IME event |
| `record_event(event)` | Record event for audit/playback |

Sub-traits: `MobileStage` (tap/swipe/input_text), `SandboxStage` (start/stop/exec/resource_usage).

## Platform Support

| Platform | Crate | Status |
|---|---|---|
| macOS | `eidolon-desktop` (Core Graphics) | Real implementation |
| Windows / Linux | `eidolon-desktop` | Fail-loud stub |
| iOS / Android | `eidolon-mobile` | Stub |
| Docker | `eidolon-sandbox` (bollard) | Feature-gated (`sandbox-docker`) |
| nanoVM | `eidolon-sandbox` (ops CLI) | Feature-gated (`sandbox-nanovm`) |
| KVM | `eidolon-sandbox` (firecracker) | Feature-gated (`sandbox-kvm`) |

## Security

- **exec command validation**: Rejects NUL bytes, newlines, shell metacharacters (`&&`, `||`, `|`, `>`, `<`, `$()`, backtick, `;`). Max 4096 bytes.
- **sandbox ID validation**: ASCII alphanumeric + `-`, `_`, `.`. Max 64 bytes. Rejects leading `-`.
- **Security gate**: `PolicySecurityGate` with capability allow-list + rate limiting.

## Audit

- **AuditEntry**: Structured append log (event type, timestamp, actor, details)
- **Integrity chain**: Optional SHA-256 hash chain over entries
- **Retention**: Age-based purge via `RetentionPolicy`
- **Stores**: `MemoryAuditStore` (default), `FileAuditStore` (feature `sandbox-audit`)

## Quick Start

```bash
cargo build
cargo test
# macOS desktop
cargo build -p eidolon-desktop
# Docker sandbox
cargo build -p eidolon-sandbox --features sandbox-docker
```

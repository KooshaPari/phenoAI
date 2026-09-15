# packages/kmobile — Absorbed into Eidolon

> **Source:** [KooshaPari/kmobile](https://github.com/KooshaPari/kmobile) (absorbed 2026-09-12)
> **Disposition:** absorb_then_archive → Eidolon

## What This Contains

This directory preserves the original KMobile codebase before its functionality
was absorbed into Eidolon's native crates:

| kmobile crate | Absorbed into (Eidolon) | Status |
|---|---|---|
| `crates/kmobile-core` (ports/adapters) | `crates/eidolon-core` + `crates/eidolon-mobile` | Superseded |
| `crates/kmobile-cli` (CLI binary) | `crates/eidolon-mobile/src/cli.rs` | Superseded |
| `crates/kmobile-api` (HTTP API) | `crates/eidolon-mobile` (integrated) | Superseded |
| `crates/kmobile-mcp` (MCP server) | `crates/eidolon-mobile` (integrated) | Superseded |
| `packages/kmobile-overlay` (FFI/IPC bridge) | `crates/eidolon-mobile/src/kmobile_bridge.rs` | Superseded |
| `src/` (standalone bin, device_basic, etc.) | `crates/eidolon-mobile/` + `crates/eidolon-desktop/` | Superseded |

## Structure

```
packages/kmobile/
├── crates/
│   ├── kmobile-core/     # Original port traits (DevicePort, MaterialPort, etc.)
│   ├── kmobile-cli/      # CLI with adapters (device, material, simulator, etc.)
│   ├── kmobile-api/      # HTTP API server
│   └── kmobile-mcp/      # MCP server implementation
├── packages/
│   └── kmobile-overlay/  # FFI/IPC bridge (kmobile-bridge crate)
├── src/                  # Original standalone implementations
│   ├── device_basic.rs   # Basic device operations
│   ├── device_bridge.rs  # Device bridge (871 lines)
│   ├── hardware_emulator.rs  # Hardware emulation (741 lines)
│   ├── mcp.rs            # MCP protocol (706 lines)
│   ├── project.rs        # Project management
│   ├── simulator_basic.rs # Simulator basics
│   ├── testing.rs        # Test framework
│   └── ...
├── docs/                 # Original kmobile documentation
├── Cargo.toml            # Original workspace config (NOT in Eidolon workspace)
├── Cargo.lock
└── kmobile.toml          # KMobile-specific config
```

## Migration Path

The canonical mobile implementation now lives in:

- **`crates/eidolon-mobile/`** — iOS (xcrun, XcuiBridge) + Android (adb, UiAutomator2, Appium)
- **`crates/eidolon-desktop/`** — macOS, Linux, Windows desktop automation
- **`crates/eidolon-core/`** — Core traits, events, input, viewport

The `kmobile_bridge.rs` module in `eidolon-mobile` provides the bridge between
Eidolon's DeviceManager and kmobile's original DevicePort trait shape.

## Why This Archive Exists

Per the Phenotype disposition register, kmobile was absorbed into Eidolon.
This archive preserves the original code for reference, port trait documentation,
and historical context. It is **not** part of the Eidolon workspace build.

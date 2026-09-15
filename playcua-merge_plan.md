# PlayCua + bare-cua Merge Plan

> **Task:** L4 #70 — PlayCua + bare-cua merge
> **Branch:** `chore/l4-70-playcua-bare-cua-merge-2026-06-11`
> **Date:** 2026-06-11
> **Status:** Phase 2 — scaffolding & documentation

---

## (a) What bare-cua provides (minimal CUA runtime)

`bare-cua` was a heavy fork of [trycua/cua](https://github.com/trycua/cua) that stripped the VM layer and replaced the `computer-server` with a **native Rust binary** communicating via **stdio JSON-RPC 2.0**.

### Core deliverables

| Component | Description |
|-----------|-------------|
| `native/src/domain/` | Pure Rust types — `Frame`, `Key`, `WindowInfo`, `ProcessHandle`, `DiffResult`, … |
| `native/src/ports/` | Async trait interfaces — `CapturePort`, `InputPort`, `WindowPort`, `ProcessPort`, `AnalysisPort` |
| `native/src/adapters/` | Platform-specific implementations: `xcap.rs` (cross-platform fallback), `enigo.rs` (cross-platform fallback), `windows/`, `linux/`, `macos/` |
| `native/src/ipc/` | Wire types (`mod_types.rs`) + dispatcher (`dispatcher.rs`) — routes JSON-RPC methods to port calls |
| `native/src/app/` | DI wiring — selects adapters via `cfg(target_os)`, builds the `Dispatcher` |
| `native/src/plugins/` | `MethodPlugin` trait + `PluginRegistry` — register additional JSON-RPC methods without touching core dispatcher |
| `native/src/main.rs` | IPC loop — reads NDJSON from stdin, dispatches, writes NDJSON to stdout |
| `contracts/openrpc.json` | Full OpenRPC 1.2.6 spec (14 methods) |
| `python/` | Python package (`bare_cua`) + pytest suite |
| `bindings/` | C# bindings |

### Design principles

1. **Hexagonal architecture (Ports & Adapters)** — domain types are pure Rust structs with zero external deps; ports are async traits; adapters are swappable.
2. **SOLID** — each adapter does one thing; the dispatcher depends on abstractions.
3. **KISS / DRY** — `xcap` and `enigo` are thin wrappers; platform-specific paths add only what the fallback cannot provide.
4. **Contract-first (OpenRPC 1.2.6)** — the full API is documented before any code ships.
5. **Polyglot / polyrepo** — Python bindings, C# bindings; each language calls the same binary over stdio.
6. **Plugin system** — additional JSON-RPC methods via `MethodPlugin` without touching core dispatcher.
7. **Observability-first** — structured JSON logs to stderr; every adapter method is instrumented with `#[tracing::instrument]`.

### Historical snapshot

`bare-cua` is now **frozen at commit `e9b44d4`** (the 2026-06-08 snapshot). The standalone repository is deprecated and no longer maintained. It is scheduled to be archived on 2026-09-01.

---

## (b) What PlayCua provides (full CUA stack)

PlayCua is a **strict superset** of bare-cua. It absorbed all bare-cua functionality and added the surrounding work that never lived in the bare-cua repo.

### Core deliverables (inherited from bare-cua)

All bare-cua functionality above is present in PlayCua, renamed as `playcua-native`:

| bare-cua name | PlayCua name |
|---------------|--------------|
| `bare-cua-native` | `playcua-native` |
| `bare-cua-cli` | `playcua-cli` |
| `bare-cua-mcp` | `playcua-mcp` |
| `bare_cua` (Python) | `playcua` (Python) |
| `bare_cua_native` (lib) | `playcua_native` (lib) |

### Additional deliverables (PlayCua-only)

| Component | Description |
|-----------|-------------|
| `native/src/modality/` | 5-modality registry: `native` / `sandbox` / `nvms` / `wsl` / `container` |
| `native/src/bin/playcua-cli.rs` | CLI binary (was `bare-cua-cli`) |
| `native/src/bin/playcua-mcp.rs` | MCP server binary (was `bare-cua-mcp`) |
| `native/src/mcp_server.rs` | MCP server implementation (rmcp 1.x SDK) |
| `native/src/socket.rs` | Unix-socket daemon |
| `native/examples/echo_plugin.rs` | Plugin example |
| `docs/adr/` | Architecture Decision Records |
| `docs/skill-sdk.md` | Skill SDK documentation |
| `crates/port-renderer/` | L4 #61 — `Renderer` port trait |
| `crates/port-window-mgr/` | L4 #61 — `WindowManager` port trait |
| `crates/port-input/` | L4 #61 — `InputSource` port trait |
| `crates/playcua-app/` | L4 #61 — composition root crate |
| `crates/playcua-bare/` | **L4 #70 (this task)** — scaffold crate for bare-cua core types |

### Workspace layout

```
PlayCua/
  Cargo.toml          # Workspace root (members = ["native"])
  native/
    Cargo.toml        # playcua-native crate (bin + lib)
    src/
      domain/
      ports/
      adapters/
      ipc/
      app/
      plugins/
      modality/
      bin/
      lib.rs
      main.rs
      mcp_server.rs
      socket.rs
  python/
  contracts/
  bindings/
  docs/
  crates/
    port-renderer/
    port-window-mgr/
    port-input/
    playcua-app/
    playcua-bare/     # ← new in L4 #70
```

---

## (c) Merge strategy

### Phase 1 (completed — 2026-06-08 to 2026-06-10)

- PlayCua absorbed bare-cua's core Rust crate as `playcua-native`.
- Binaries renamed: `bare-cua-cli` → `playcua-cli`, `bare-cua-mcp` → `playcua-mcp`.
- Python package renamed: `bare_cua` → `playcua`.
- `DEPRECATED_BARE_CUA.md` added to PlayCua root documenting the migration path.
- bare-cua repository frozen at `e9b44d4` and marked as deprecated.

### Phase 2 (this task — L4 #70)

- **Scaffold** a new `playcua-bare` crate under `PlayCua/crates/`.
- This crate will eventually hold the **bare-cua core types** (the domain types, port traits, and IPC wire types that originally lived in `bare-cua/native/src/`).
- The crate is intentionally **standalone** (not a workspace member) so it can be developed independently.
- The crate is currently a **stub** — it re-exports placeholder types from `src/lib.rs`. The actual file migration from bare-cua will happen in a follow-up task.

### Phase 3 (future — not in scope for L4 #70)

- Move the actual bare-cua core source files into `playcua-bare/src/`.
- Update `playcua-native` to depend on `playcua-bare` for domain types instead of owning them inline.
- This decouples the **domain model** from the **runtime binary**, allowing downstream crates (e.g., `playcua-app`, helioscli) to depend on the types without pulling in the entire `playcua-native` crate.

### bare-cua repository status

- `bare-cua` is now a **read-only archive**.
- No new features, bug fixes, security patches, or pull requests are accepted.
- The repository is scheduled for archival on **2026-09-01**.
- Historical consumers should migrate to PlayCua.

---

## (d) File mapping table

| bare-cua path (historical) | PlayCua target path (Phase 3) | Status |
|----------------------------|-------------------------------|--------|
| `bare-cua/native/src/domain/mod.rs` | `PlayCua/crates/playcua-bare/src/domain/mod.rs` | Planned |
| `bare-cua/native/src/domain/frame.rs` | `PlayCua/crates/playcua-bare/src/domain/frame.rs` | Planned |
| `bare-cua/native/src/domain/key.rs` | `PlayCua/crates/playcua-bare/src/domain/key.rs` | Planned |
| `bare-cua/native/src/domain/window.rs` | `PlayCua/crates/playcua-bare/src/domain/window.rs` | Planned |
| `bare-cua/native/src/ports/mod.rs` | `PlayCua/crates/playcua-bare/src/ports/mod.rs` | Planned |
| `bare-cua/native/src/ports/capture.rs` | `PlayCua/crates/playcua-bare/src/ports/capture.rs` | Planned |
| `bare-cua/native/src/ports/input.rs` | `PlayCua/crates/playcua-bare/src/ports/input.rs` | Planned |
| `bare-cua/native/src/ports/window.rs` | `PlayCua/crates/playcua-bare/src/ports/window.rs` | Planned |
| `bare-cua/native/src/ipc/mod.rs` | `PlayCua/crates/playcua-bare/src/ipc/mod.rs` | Planned |
| `bare-cua/native/src/ipc/mod_types.rs` | `PlayCua/crates/playcua-bare/src/ipc/mod_types.rs` | Planned |
| `bare-cua/native/src/ipc/dispatcher.rs` | `PlayCua/crates/playcua-bare/src/ipc/dispatcher.rs` | Planned |
| `bare-cua/native/src/app/mod.rs` | `PlayCua/crates/playcua-bare/src/app/mod.rs` | Planned |
| `bare-cua/native/src/plugins/mod.rs` | `PlayCua/crates/playcua-bare/src/plugins/mod.rs` | Planned |
| `bare-cua/native/src/main.rs` | `PlayCua/native/src/main.rs` | **Already merged** |
| `bare-cua/native/src/lib.rs` | `PlayCua/native/src/lib.rs` | **Already merged** |
| `bare-cua/contracts/openrpc.json` | `PlayCua/contracts/openrpc.json` | **Already merged** |
| `bare-cua/python/bare_cua/` | `PlayCua/python/playcua/` | **Already merged** |
| `bare-cua/README.md` | `PlayCua/DEPRECATED_BARE_CUA.md` | **Already merged** |

---

## Notes

- The `playcua-bare` crate created in this task is a **scaffolding stub**. No actual files are moved from bare-cua in Phase 2.
- Verification: `cargo check --manifest-path playcua-bare/Cargo.toml` should pass on the stub (or be skipped if no real code is present yet).
- The actual code migration will be validated in Phase 3 with `cargo test`, `cargo clippy`, and `cargo check` across the full workspace.

# Virtual display manager (sandbox)

PlayCua / KDesktopVirt virtual display patterns live in
`eidolon-sandbox::virtual_display`. Compose with
[`PlayCuaDispatcher`](../../crates/eidolon-sandbox/src/playcua_dispatcher.rs) by
exporting `DISPLAY` from a started session.

## What is shipped

| Path | Status | Code when unavailable |
|------|--------|------------------------|
| Host probes (`Xvfb`, x11vnc, `Xvnc`, Weston) | Always-on | — |
| Hermetic `plan_xvfb` (argv, no spawn) | Linux + `Xvfb` on `PATH` | `EIDOLON_SANDBOX_XVFB_MISSING` |
| Live Xvfb spawn | Linux + feature `sandbox-virtual-display` + `XVFB_INTEGRATION=1` | `EIDOLON_SANDBOX_VIRTUAL_DISPLAY_STUB` |
| VNC attach (x11vnc / TigerVNC) | Probes only | `EIDOLON_SANDBOX_VNC_UNSUPPORTED` |
| Wayland compositor isolation | Probes only | `EIDOLON_SANDBOX_WAYLAND_COMPOSITOR_UNSUPPORTED` |
| macOS / Windows | Fail-loud | `EIDOLON_SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED` |

Host Wayland desktop automation (portals, screenshot, input) is **`eidolon-desktop`**
— not this module.

## Environment

| Variable | Purpose |
|----------|---------|
| `EIDOLON_XVFB` | Override `Xvfb` binary path |
| `EIDOLON_X11VNC` | Override x11vnc path (probe only today) |
| `EIDOLON_XVNC` | Override TigerVNC `Xvnc` path (probe only) |
| `EIDOLON_WESTON` | Override compositor binary (probe only) |
| `EIDOLON_VIRTUAL_DISPLAY_NUM` | X display number (default `99` → `:99`) |
| `XVFB_INTEGRATION=1` | Allow live Xvfb child spawn |

## Example (Rust)

```rust
use eidolon_sandbox::{VirtualDisplayConfig, VirtualDisplayManager};

let mgr = VirtualDisplayManager::with_defaults()?;
let plan = mgr.plan_xvfb()?; // hermetic — no spawn
// Live: enable feature `sandbox-virtual-display`, set XVFB_INTEGRATION=1
let handle = mgr.start()?;
let (key, display) = handle.display_env();
std::env::set_var(key, display);
mgr.stop()?;
```

## Tests

- `crates/eidolon-sandbox/tests/virtual_display.rs` — hermetic contracts on all platforms
- Unit tests under `crates/eidolon-sandbox/src/virtual_display/`

# Windows desktop capture (DXGI / GDI)

Honesty checklist for the A+ Windows screenshot path.

## Status

| Piece | Status |
|-------|--------|
| Preference plan (`auto` → DXGI then GDI) | ✅ hermetic (`capture_plan` / unit tests) |
| DXGI Desktop Duplication | ✅ `desktop-windows` on `target_os=windows` |
| GDI BitBlt fallback | ✅ same feature |
| Top-down BMP (`biHeight < 0`) | ✅ shared `windows/bmp.rs` (matches Linux) |
| Live smoke (this machine) | ⚠️ gated — Windows host only |

## Env

| Variable | Values | Role |
|----------|--------|------|
| `EIDOLON_DESKTOP_WIN_CAPTURE` | `auto` (default) / `dxgi` / `gdi` | Backend preference |
| `EIDOLON_DESKTOP_WIN_CAPTURE_SMOKE` | `1` to enable | Live smoke gate |
| `EIDOLON_DESKTOP_ALLOW_ACTIONS` | `1` to allow | Screenshot write + input |

Miss / wrong host → fail-loud `EIDOLON_DESKTOP_WIN_CAPTURE_UNAVAILABLE`.
Feature off / stub → `EIDOLON_DESKTOP_WIN_STUB`.

## Live smoke (Windows only)

```powershell
$env:EIDOLON_DESKTOP_WIN_CAPTURE_SMOKE = "1"
$env:EIDOLON_DESKTOP_ALLOW_ACTIONS = "1"
$env:EIDOLON_DESKTOP_WIN_CAPTURE = "auto"   # or dxgi / gdi
cargo test -p eidolon-desktop --features desktop-windows --locked `
  --test scaffolding windows_capture_smoke -- --nocapture
```

Off-Windows (macOS/Linux CI):

```bash
# Hermetic: host gate fails loud without inventing a capture
cargo test -p eidolon-desktop --locked capture_host_gate_matches_target
cargo test -p eidolon-desktop --locked windows_capture_smoke_fails_loud_off_windows
```

Setting `EIDOLON_DESKTOP_WIN_CAPTURE_SMOKE=1` on macOS/Linux still fails loud —
there is no silent skip disguised as success.

## Checklist

- [ ] Built with `--features desktop-windows` (alias `desktop-windows-dxgi`) on Windows
- [ ] Session has an interactive desktop (Desktop Duplication needs a console session)
- [ ] `EIDOLON_DESKTOP_WIN_CAPTURE_SMOKE=1` and `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`
- [ ] Prefer `auto` first; isolate with `dxgi` or `gdi` when debugging
- [ ] BMP header shows negative `biHeight` (top-down)

## Related

- `crates/eidolon-desktop/src/windows/capture.rs` — preference + smoke gates
- `crates/eidolon-desktop/src/windows/dxgi.rs` / `gdi.rs` / `bmp.rs`
- `docs/EXTRACTION_PLAN.md` Phase 2

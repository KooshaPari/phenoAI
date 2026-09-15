# Linux live smoke harnesses (USER ns + Wayland inject/restore)

Honesty checklist for env-gated Linux integration smokes.

## Status

| Piece | Status |
|-------|--------|
| USER ns identity via `EIDOLON_SANDBOX_NS_USER=1` + `plan_from_policy` | ✅ smoke harness (`enforcement_linux.rs`) |
| USER ns multi-range via `newuidmap`/`newgidmap` | ✅ smoke harness — success or fail-loud `EIDOLON_SANDBOX_NAMESPACES_UNSUPPORTED` |
| Wayland inject smoke gates (hermetic) | ✅ `linux/wayland_smoke.rs` — all targets |
| Wayland live pointer/text/restore | ⚠️ gated — Linux + `WAYLAND_DISPLAY` + portal only |
| macOS CI default test run | ✅ hermetic gates stay green (no live Wayland claimed) |

## USER namespace smoke

### Env

| Variable | Values | Role |
|----------|--------|------|
| `NAMESPACES_INTEGRATION` | `1` | Enable ignored live namespace tests |
| `EIDOLON_SANDBOX_NS_USER` | `1` | Plan USER ns + uid/gid maps |
| `EIDOLON_SANDBOX_NS_USER_SUBIDS` | `1` (optional) | Read `/etc/subuid` + `/etc/subgid` |
| `EIDOLON_SANDBOX_NS_UID_MAP` / `GID_MAP` | `inside:outside:count` | Explicit multi-range override |

Off-Linux with integration env set → tests **panic** (fail-loud). Multi-range on Linux
without `newuidmap`/`newgidmap` → **panic** (tools missing).

### Live (Linux or Docker `--privileged`)

```bash
export PATH="/bin:/usr/bin:/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"

NAMESPACES_INTEGRATION=1 EIDOLON_SANDBOX_NS_USER=1 \
  cargo test -p eidolon-sandbox --features sandbox-namespaces --locked \
  --test enforcement_linux user_ns -- --ignored --nocapture
```

macOS host via privileged amd64 container:

```bash
export PATH="/bin:/usr/bin:/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
./scripts/linux-user-ns-smoke-docker.sh
```

## Wayland inject / restore smoke

### Env

| Variable | Values | Role |
|----------|--------|------|
| `EIDOLON_DESKTOP_WAYLAND_SMOKE` | `1` | Live inject smoke gate |
| `EIDOLON_DESKTOP_ALLOW_ACTIONS` | `1` | Pointer / text inject |
| `WAYLAND_DISPLAY` | set | Pure-Wayland session (unset `DISPLAY` for portal path) |
| `EIDOLON_DESKTOP_WAYLAND_RESTORE` | `1` (optional) | Restore-token persistence smoke |

Portal miss/deny → `EIDOLON_DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`.
Device/stream gap → `EIDOLON_DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED`.
Restore store I/O → `EIDOLON_DESKTOP_LINUX_WAYLAND_RESTORE_IO`.

### Hermetic (macOS / CI — no live Wayland)

```bash
export PATH="/bin:/usr/bin:/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"

cargo test -p eidolon-desktop --locked \
  wayland_smoke_fails_loud_off_linux_or_without_wayland
cargo test -p eidolon-desktop --features desktop-linux --locked \
  wayland_host_gate_matches_target
```

Setting `EIDOLON_DESKTOP_WAYLAND_SMOKE=1` on macOS still fails loud — there is no
silent skip disguised as success.

### Live (Linux + compositor + xdg-desktop-portal)

```bash
export WAYLAND_DISPLAY=wayland-0
unset DISPLAY
export EIDOLON_DESKTOP_WAYLAND_SMOKE=1
export EIDOLON_DESKTOP_ALLOW_ACTIONS=1
# optional:
export EIDOLON_DESKTOP_WAYLAND_RESTORE=1

cargo test -p eidolon-desktop --features desktop-linux --locked \
  --test scaffolding wayland_inject_smoke -- --nocapture
```

**Do not claim live Wayland** unless this command ran on a host with a running
compositor, session bus, and portal — portal deny codes are an expected outcome
in headless CI.

## Checklist

- [ ] Built with `--features sandbox-namespaces` (USER ns) or `desktop-linux` (Wayland)
- [ ] Integration / smoke env vars set explicitly
- [ ] USER ns multi-range: `newuidmap` + `newgidmap` installed (shadow-utils)
- [ ] Wayland live: `WAYLAND_DISPLAY` set; portal available or deny codes documented
- [ ] macOS default `cargo test` stays green without integration env

## Related

- `crates/eidolon-sandbox/tests/enforcement_linux.rs` — USER ns live smokes
- `crates/eidolon-desktop/src/linux/wayland_smoke.rs` — Wayland smoke gates
- `crates/eidolon-desktop/tests/scaffolding.rs` — cross-target hermetic + Linux live
- `docs/guides/windows-desktop-capture.md` — parallel pattern for Windows capture smoke

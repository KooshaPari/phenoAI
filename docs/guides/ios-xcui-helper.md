# iOS XCUI helper

Eidolon speaks the [`BundleXcuiBridge`](../../crates/eidolon-mobile/src/ios/xcui_bridge.rs) CLI contract via two in-repo helpers:

1. **Preferred:** in-tree XCUITest host — `native/ios/EidolonXcuiHelper/`
2. **Fallback:** Rust `eidolon-xcui-helper` (AppleScript / simctl scaffold)

## Honesty

| Backend | tap / swipe / text | viewport | Notes |
|---------|--------------------|----------|-------|
| **XCUITest** (`eidolon-xcui-xctest`) | Device points via `XCUICoordinate` | `XCUIScreen.main` | `text` needs keyboard focus; fails loud otherwise |
| **Rust helper** | macOS: `simctl` UDID check + `osascript` **screen points** | Known `deviceTypeIdentifier` map | Not device-pixel XCUI |
| Non-macOS | Fail loud | Fail loud | |
| Missing / Shutdown Simulator | Fail loud | Fail loud | Never invent success |

Unsupported ops fail loud. Do **not** unarchive kmobile for routine work. Vendored UIA2 APKs remain separate; **Appium-compatible session client** is landed (`mobile-uia2` / `mobile-appium`) — Electron Appium Desktop GUI stays out of scope.

## CLI contract

```text
<helper> tap --udid <id> --x <n> --y <n>
<helper> swipe --udid <id> --x1 <n> --y1 <n> --x2 <n> --y2 <n>
<helper> text --udid <id> --value <str>
<helper> viewport --udid <id>   # stdout: WIDTH HEIGHT [SCALE]
```

## Build — XCUITest (preferred)

```bash
cd native/ios/EidolonXcuiHelper
# Boot a Simulator first
swift Scripts/build-for-testing.swift          # or pass UDID / EIDOLON_MOBILE_DEVICE
# → build/eidolon-xcui-xctest + build/.eidolon-xcui-built
# derived data stays under ./build (never /tmp)
```

See `native/ios/EidolonXcuiHelper/README.md` for `xcodebuild` equivalents and XcodeGen regen.

## Build — Rust fallback

```bash
cargo build -p eidolon-mobile --features mobile-xcui-helper --bin eidolon-xcui-helper --locked
```

Feature `mobile-xcui-helper` only gates the **Rust binary**. Library parse/discovery/execute (including Xcode discovery) live always-on under `eidolon_mobile::ios::xcui_helper`.

## Discovery (bridge)

`resolve_xcui_bridge()` order:

1. **`EIDOLON_IOS_XCUI_BUNDLE`** — explicit path wins when it is an existing file
2. **In-tree XCUITest runner** — `native/ios/EidolonXcuiHelper/build/eidolon-xcui-xctest` when built (`build/.eidolon-xcui-built` or Products/); override project dir with `EIDOLON_IOS_XCUI_XCODE_DIR`
3. **Rust `eidolon-xcui-helper`** — `CARGO_BIN_EXE_*` → sibling of `current_exe()` → `PATH`
4. **AppleScript** — only when `EIDOLON_IOS_ALLOW_APPLESCRIPT=1`
5. **`MissingXcuiBridge`** — `EIDOLON_MOBILE_IOS_XCUI_UNAVAILABLE`

## Tests

```bash
# Hermetic (no Xcode required):
cargo test -p eidolon-mobile --locked
cargo test -p eidolon-mobile --locked --features mobile-ios,mobile-xcui-helper

# Optional live XCUI / xcodebuild (ignored by default):
EIDOLON_XCUI_XCODE_INTEGRATION=1 EIDOLON_MOBILE_DEVICE=<udid> \
  cargo test -p eidolon-mobile --locked --features mobile-ios,mobile-xcui-helper \
  -- --ignored
```

Optional live AppleScript path: `IOS_MOBILE_INTEGRATION=1` + booted sim.

## Related

- `native/ios/EidolonXcuiHelper/README.md` — Xcode project layout
- `docs/EXTRACTION_PLAN.md` — XCUI project landed; Appium-compatible session client landed (`mobile-uia2` / `mobile-appium`); Electron Appium Desktop GUI out of scope
- Feature drivers: `mobile-ios` (`XcrunIosDriver`)
- `docs/guides/appium-session.md` — Appium HTTP session surface

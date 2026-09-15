# Eidolon XCUI Helper (in-tree Xcode / XCUITest)

Real XCUITest host for `BundleXcuiBridge`. Preferred over the Rust
`eidolon-xcui-helper` AppleScript scaffold **when built**.

## Honesty

| Op | XCUI behavior |
|----|----------------|
| `tap` / `swipe` | `XCUICoordinate` offsets in **device points** (host app or SpringBoard) |
| `text` | `XCUIApplication.typeText` — needs keyboard / first responder; fails loud otherwise |
| `viewport` | `XCUIScreen.main.bounds` + scale → `WIDTH HEIGHT SCALE` |
| Unsupported op | Fail loud |
| Non-macOS | Runner / build scripts fail loud |

Not a full accessibility query / element-finder suite. Vendored Appium Desktop and UIA2 APKs remain out of scope. Do **not** unarchive kmobile.

## Layout

```text
native/ios/EidolonXcuiHelper/
  EidolonXcuiHelper.xcodeproj/     # committed (regenerate via xcodegen if needed)
  EidolonXcuiHelper/               # minimal UI-test host app
  EidolonXcuiHelperUITests/        # XCUITest action runner
  Scripts/eidolon-xcui-xctest      # argv contract → xcodebuild test-without-building
  Scripts/build-for-testing.swift  # build-for-testing → ./build
  build/                           # derived data + stamp + installed runner (gitignored)
```

## Build (macOS + Xcode)

```bash
cd native/ios/EidolonXcuiHelper
# Boot a Simulator, then:
chmod +x Scripts/eidolon-xcui-xctest Scripts/build-for-testing.swift
swift Scripts/build-for-testing.swift            # or pass UDID / EIDOLON_MOBILE_DEVICE
# → prints build/eidolon-xcui-xctest and writes build/.eidolon-xcui-built
```

Equivalent `xcodebuild`:

```bash
xcodebuild build-for-testing \
  -project EidolonXcuiHelper.xcodeproj \
  -scheme EidolonXcuiHelper \
  -destination "id=$UDID" \
  -derivedDataPath "$PWD/build" \
  CODE_SIGNING_ALLOWED=NO
cp Scripts/eidolon-xcui-xctest build/eidolon-xcui-xctest
chmod +x build/eidolon-xcui-xctest
date > build/.eidolon-xcui-built
```

Regenerate the project (optional):

```bash
xcodegen generate   # requires project.yml + xcodegen
```

## CLI contract

Same as `eidolon-xcui-helper` / `BundleXcuiBridge`:

```text
build/eidolon-xcui-xctest tap --udid <id> --x <n> --y <n>
build/eidolon-xcui-xctest swipe --udid <id> --x1 <n> --y1 <n> --x2 <n> --y2 <n>
build/eidolon-xcui-xctest text --udid <id> --value <str>
build/eidolon-xcui-xctest viewport --udid <id>
```

The runner injects `EIDOLON_XCUI_REQUEST_JSON` via a temporary `.xctestrun` under
`./build` (Simulator UITests cannot read host paths) and scrapes
`EIDOLON_XCUI_RESULT:…` from `xcodebuild` logs.
## Discovery (Rust bridge)

`resolve_xcui_bridge()` order:

1. **`EIDOLON_IOS_XCUI_BUNDLE`** — explicit path wins when it is an existing file
2. **In-tree XCUI runner** — `native/ios/EidolonXcuiHelper/build/eidolon-xcui-xctest` when `build/.eidolon-xcui-built` exists (or `EIDOLON_IOS_XCUI_XCODE_DIR`)
3. **Rust `eidolon-xcui-helper`** — AppleScript/simctl fallback (`mobile-xcui-helper`)
4. **AppleScript** — `EIDOLON_IOS_ALLOW_APPLESCRIPT=1`
5. **`MissingXcuiBridge`**

## Integration gate

Hermetic Rust tests must **not** invoke `xcodebuild`. Optional live:

```bash
EIDOLON_XCUI_XCODE_INTEGRATION=1 cargo test -p eidolon-mobile --locked \
  --features mobile-ios,mobile-xcui-helper -- --ignored
```

## Related

- `docs/guides/ios-xcui-helper.md`
- `docs/EXTRACTION_PLAN.md`

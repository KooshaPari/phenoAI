# Appium UiAutomator2 APKs (`eidolon-fetch-uia2`)

Pinned official Appium UiAutomator2 **server** + **instrumentation** APKs
(Apache-2.0). Binaries are ~17 MB and live in a durable SHA-256-verified cache
— not in git.

## Honesty

| Path | Behavior |
|------|----------|
| Env override | `EIDOLON_UIA2_APK` + `EIDOLON_UIA2_TEST_APK` win when both exist |
| Checkout assets | Optional local drop under `crates/eidolon-mobile/assets/uia2/` |
| Durable cache | `EIDOLON_UIA2_CACHE` or `~/.cache/eidolon/uia2/<version>/` |
| Missing | Fail loud `EIDOLON_MOBILE_UIA2_UNAVAILABLE` (never silent empty install) |

`/tmp` is rejected as a cache root. Do **not** unarchive kmobile for routine UIA2 work.

Pinned release + digests: [`assets/uia2/manifest.json`](../../crates/eidolon-mobile/assets/uia2/manifest.json).
Attribution: [`NOTICE`](../../crates/eidolon-mobile/assets/uia2/NOTICE) + upstream [`LICENSE`](../../crates/eidolon-mobile/assets/uia2/LICENSE).

## Fetch

```bash
cargo run -p eidolon-mobile --features mobile-uia2 --bin eidolon-fetch-uia2 --locked
```

Requires system `curl` for HTTPS GitHub downloads (SHA-256 verified after write).
Library happy path: `Uia2ApkPaths::ensure()` (resolve, or download + verify into cache).

## Resolution order

1. **`EIDOLON_UIA2_APK` + `EIDOLON_UIA2_TEST_APK`**
2. **Checkout `assets/uia2/*.apk`** (hash-verified)
3. **Durable cache** (hash-verified)
4. **Fail loud** — run fetch or set env

## Tests

```bash
cargo test -p eidolon-mobile --locked --features mobile-android,mobile-uia2,mobile-appium
```

Hermetic tests cover empty-root fail-loud and env override. Optional live:
`ANDROID_UIA2_INTEGRATION=1` + device + `EIDOLON_MOBILE_ALLOW_ACTIONS=1`.

## Related

- `docs/EXTRACTION_PLAN.md` — T1 mobile status
- `docs/guides/appium-session.md` — Appium-compatible session client + probe
- Feature: `mobile-uia2` (`Uia2Server` + `Uia2HttpClient` + fetch bin)
- Feature: `mobile-appium` (optional Appium server probe; GUI out of scope)

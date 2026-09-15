# Appium-compatible session client (`mobile-uia2` / `mobile-appium`)

Eidolon ships an **Appium-compatible HTTP session client** plus an optional
Inspector launcher / local dashboard (`mobile-appium-desktop`). Talks to
Appium UiAutomator2 on `:6790` (after `Uia2Server` forward) or an Appium
server on `:4723`.

## Honesty

| Claim | Status |
|-------|--------|
| W3C / Appium session verbs over HTTP | ✅ `Uia2HttpClient` / `AppiumSessionClient` |
| UIA2 APK lifecycle | ✅ `Uia2Server` + pinned cache (`eidolon-fetch-uia2`) |
| Optional Appium server probe | ✅ `mobile-appium` (`APPIUM_HOME` / `appium` / `EIDOLON_APPIUM_URL`) |
| Inspector launcher + local dashboard | ✅ `mobile-appium-desktop` / `eidolon-appium-desktop` |
| Full Electron Appium Desktop fork | ❌ out of scope |
| kmobile unarchive | ❌ do not |

## Session endpoints (HTTP)

| Verb | Path | Gate |
|------|------|------|
| status | `GET /status` | ungated |
| create/delete session | `POST`/`DELETE /session[/:id]` | actions |
| findElement(s) | `POST …/element(s)` | ungated |
| click / clear / value / text | `…/element/:id/…` | click/clear/value gated |
| page source | `GET …/source` | ungated |
| windows | `GET`/`POST`/`DELETE …/window[…]` | switch/close/set gated |
| contexts | `GET`/`POST …/context(s)` | set gated |
| actions (pointer) | `POST`/`DELETE …/actions` | gated |
| screenshot | `GET …/screenshot` | ungated |
| timeouts | `GET`/`POST …/timeouts` | set gated |

Unsupported vendor extensions → fail-loud `EIDOLON_MOBILE_UIA2_UNAVAILABLE`
via `Uia2HttpClient::unsupported`.

## Appium probe (`mobile-appium`)

```bash
# Hermetic when appium absent
cargo test -p eidolon-mobile --locked --features mobile-android,mobile-uia2,mobile-appium
```

| Env | Role |
|-----|------|
| `EIDOLON_APPIUM_URL` | HTTP base (e.g. `http://127.0.0.1:4723`) |
| `APPIUM_HOME` | Install home with `appium` bin |
| `EIDOLON_APPIUM` | Explicit CLI path |

Missing tools: `discover()` / `appium_tools_ready()` stay false (hermetic).
Required use: `require_appium_ready` / `http_status` →
`EIDOLON_MOBILE_APPIUM_UNAVAILABLE`.

## Related

- `docs/guides/uia2-apks.md` — APK fetch/cache
- `docs/EXTRACTION_PLAN.md` — T1 mobile honesty

# Vendored Appium UiAutomator2 APKs

Official Appium UiAutomator2 **server** + **instrumentation** APKs (Apache-2.0).
Binary APKs are ~17 MB and are **not** committed to git; they are fetched into a
durable cache with pinned SHA-256 verification.

## Resolution order

1. **Env override** — `EIDOLON_UIA2_APK` + `EIDOLON_UIA2_TEST_APK` (both must exist)
2. **Checkout assets** — `crates/eidolon-mobile/assets/uia2/*.apk` (optional local drop)
3. **Durable cache** — `$EIDOLON_UIA2_CACHE` or `~/.cache/eidolon/uia2/<version>/`
4. **Fail loud** — `EIDOLON_MOBILE_UIA2_UNAVAILABLE` (run fetch, or set env)

`/tmp` is rejected as a cache root.

## Fetch (happy path)

```bash
cargo run -p eidolon-mobile --features mobile-uia2 --bin eidolon-fetch-uia2 --locked
# or ensure via library: Uia2ApkPaths::ensure() (downloads if cache miss)
# requires system curl for HTTPS; SHA-256 verified after download
```

Pinned version and digests: [`manifest.json`](./manifest.json). Attribution:
[`NOTICE`](./NOTICE), [`LICENSE`](./LICENSE) (upstream Apache-2.0).

## Do not

- Unarchive kmobile for routine UIA2 work
- Skip SHA-256 verification
- Silently install empty / missing APKs

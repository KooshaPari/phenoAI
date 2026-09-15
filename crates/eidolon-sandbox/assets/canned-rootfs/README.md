# Canned rootfs assets

| Path | Purpose |
|------|---------|
| `minimal-tree/` | Hermetic stub guest tree fixture (not a full Linux userland) |
| `release-manifest.json` | Checkout pin for published GH Ext4 disk `rootfs-v0.1.0` (UIA2-shaped) |

## Honesty

- Canned pipeline (`eidolon-canned-rootfs`) and release staging (`eidolon-release-rootfs`) are shipped.
- A **default published** GitHub release Ext4 asset is **not** shipped in this checkout —
  `published: false` / empty URL+SHA. Do not treat this folder as containing a disk image.
- After a real one-shot `gh release upload`, either:
  1. Fill `release-manifest.json` + `pack::release_asset` pin constants, **or**
  2. Export `EIDOLON_ROOTFS_ASSET_URL` + `EIDOLON_ROOTFS_ASSET_SHA256` (operator pin; no fake constants).
- Download: `eidolon-fetch-rootfs` / `ensure_rootfs_release` — fail-loud when neither checkout
  nor env pin is set (`EIDOLON_SANDBOX_ROOTFS_RELEASE_UNAVAILABLE`).

See `docs/guides/gh-ext4-release.md` and `docs/guides/canned-rootfs.md`.

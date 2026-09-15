# Published GH Ext4 rootfs release asset

Recipe for a **versioned Ext4 disk** with `eidolon-vsock-agent`, SHA-256
checksums, and a UIA2-shaped pin/download client.

## Honesty

| Piece | Status |
|-------|--------|
| Canned bake + Ext4 pack (`eidolon-canned-rootfs`) | ✅ shipped (#113) |
| Release staging CLI (`eidolon-release-rootfs`) | ✅ shipped (#118) |
| Pin + fetch client (`eidolon-fetch-rootfs` / `pack::release_asset`) | ✅ shipped (#118) |
| Operator env pin (`EIDOLON_ROOTFS_ASSET_URL` + `SHA256`) | ✅ this change |
| **Default published GH release Ext4 asset** | ✅ `rootfs-v0.1.0` published; checkout pin filled |
| GitHub Actions auto-publish | ❌ optional `workflow_dispatch` only — **prefer manual** (Actions billing exhausted) |

Pipeline + download client landed; default pin filled for `rootfs-v0.1.0`.
Operators may override with `EIDOLON_ROOTFS_ASSET_URL` + `EIDOLON_ROOTFS_ASSET_SHA256`
or `EIDOLON_ROOTFS_IMG`. Incomplete env pairs fail loud.

**Private repos:** unauthenticated `releases/download` URLs return 404.
`eidolon-fetch-rootfs` retries via the GitHub Assets API when `GH_TOKEN` or
`GITHUB_TOKEN` is set (same token `gh auth` uses).

## Durable paths (never `/tmp`)

| Role | Default | Override |
|------|---------|----------|
| Canned build out | `target/canned-rootfs/` | `EIDOLON_CANNED_ROOTFS_OUT` |
| Release staging | `target/rootfs-release/` | `EIDOLON_ROOTFS_RELEASE_OUT` |
| Download cache | `~/.cache/eidolon/rootfs-release/<version>/` | `EIDOLON_ROOTFS_RELEASE_CACHE` |
| BYO local disk | — | `EIDOLON_ROOTFS_IMG` |
| Operator URL pin | — | `EIDOLON_ROOTFS_ASSET_URL` + `EIDOLON_ROOTFS_ASSET_SHA256` |

## Build + stage (Linux)

Needs: musl `eidolon-vsock-agent`, `mkfs.ext4` (or `virt-make-fs`),
`ROOTFS_PACK_INTEGRATION=1`, feature `sandbox-rootfs-pack`.

```bash
# 1. Guest agent (Linux)
cargo build -p eidolon-sandbox --features sandbox-vsock-agent \
  --bin eidolon-vsock-agent --target x86_64-unknown-linux-musl --locked --release
export EIDOLON_VSOCK_AGENT=target/x86_64-unknown-linux-musl/release/eidolon-vsock-agent

# 2a. One-shot: canned PackExt4 + versioned stage
ROOTFS_PACK_INTEGRATION=1 cargo run -p eidolon-sandbox \
  --features sandbox-rootfs-pack --bin eidolon-release-rootfs --locked -- \
  --build --agent "$EIDOLON_VSOCK_AGENT" --version 0.1.0 --tag rootfs-v0.1.0

# 2b. Or stage an existing canned image
ROOTFS_PACK_INTEGRATION=1 cargo run -p eidolon-sandbox \
  --features sandbox-rootfs-pack --bin eidolon-canned-rootfs --locked -- \
  --agent "$EIDOLON_VSOCK_AGENT" --mode pack_ext4
cargo run -p eidolon-sandbox --features sandbox-rootfs-pack \
  --bin eidolon-release-rootfs --locked -- \
  --from-img target/canned-rootfs/rootfs.img --version 0.1.0
```

Staging layout:

```text
target/rootfs-release/
  eidolon-canned-rootfs-0.1.0-x86_64.ext4.img
  eidolon-canned-rootfs-0.1.0-x86_64.ext4.img.sha256
  eidolon-rootfs-release-manifest.json
```

## Publish (manual `gh release` — preferred)

Actions billing on this account is exhausted; do **not** rely on CI to publish.

```bash
TAG=rootfs-v0.1.0
OUT=target/rootfs-release
IMG=$OUT/eidolon-canned-rootfs-0.1.0-x86_64.ext4.img

gh release create "$TAG" \
  --title "Eidolon canned rootfs $TAG" \
  --notes "Ext4 guest disk with eidolon-vsock-agent (canned pipeline)." \
  --draft

gh release upload "$TAG" \
  "$IMG" \
  "$IMG.sha256" \
  "$OUT/eidolon-rootfs-release-manifest.json"

# Inspect digest
cat "$IMG.sha256"
```

Then update the pin (pick one):

1. **Checkout pin** — `crates/eidolon-sandbox/assets/canned-rootfs/release-manifest.json`
   — set `published: true`, `version`, `filename`, `url`, `sha256`, `bytes`;
   and Rust constants in `unikernel/pack/release_asset.rs`:
   `ROOTFS_RELEASE_PUBLISHED`, `ROOTFS_RELEASE_VERSION`,
   `ROOTFS_RELEASE_FILENAME`, `ROOTFS_RELEASE_URL`, `ROOTFS_RELEASE_SHA256`
2. **Env pin (no checkout edit)** — export both:
   ```bash
   export EIDOLON_ROOTFS_ASSET_URL="https://github.com/KooshaPari/Eidolon/releases/download/$TAG/$IMG_BASENAME"
   export EIDOLON_ROOTFS_ASSET_SHA256="$(cut -d' ' -f1 "$IMG.sha256")"
   ```

Download URL shape:

```text
https://github.com/KooshaPari/Eidolon/releases/download/<tag>/<filename>
```

## Download / verify (after pin is published or env-pinned)

```bash
# Checkout pin published, or:
#   export EIDOLON_ROOTFS_ASSET_URL=... EIDOLON_ROOTFS_ASSET_SHA256=...
cargo run -p eidolon-sandbox --features sandbox-rootfs-pack \
  --bin eidolon-fetch-rootfs --locked
```

Library: `unikernel_pack::ensure_rootfs_release()` /
`resolve_rootfs_release()` / `effective_release_pin()`.
Order: `EIDOLON_ROOTFS_IMG` → env URL+SHA → durable cache → checkout pin.
Miss / incomplete env / bad digest → fail-loud
`EIDOLON_SANDBOX_ROOTFS_RELEASE_UNAVAILABLE`.

## Optional Actions workflow

`.github/workflows/ext4-rootfs-release.yml` is **`workflow_dispatch` only**,
`ubuntu-latest` only (no macOS/Windows). It builds/stages artifacts and uploads
a workflow artifact — it does **not** create a GitHub Release by default
(billing-safe). Prefer the manual `gh` steps above.

## Rust API

```rust
use eidolon_sandbox::unikernel_pack::{
    stage_release_asset, ReleaseAssetRequest, resolve_rootfs_release,
};

// Stage from an existing Ext4 image (checksums need sandbox-rootfs-pack):
let staged = stage_release_asset(
    &ReleaseAssetRequest::new("target/canned-rootfs/rootfs.img", "0.1.0")
        .with_checksum(true),
)?;

// After pin publish:
let img = resolve_rootfs_release()?;
```

## Tests

```bash
cargo test -p eidolon-sandbox --locked release_asset
cargo test -p eidolon-sandbox --locked --features sandbox-rootfs-pack release_asset
```

## Related

- `docs/guides/canned-rootfs.md` — canned tree / Ext4 bake
- `docs/guides/vsock-guest-agent.md` — agent build + bake
- `docs/reference/rootfs-pack.md` — pack methods + env gates
- `crates/eidolon-mobile/assets/uia2/` — pin/fetch pattern this mirrors

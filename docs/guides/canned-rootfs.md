# Canned rootfs with `eidolon-vsock-agent`

First-class recipe for a durable guest rootfs tree (and optional Ext4 disk)
with the vsock NDJSON agent preinstalled at `/usr/local/bin/eidolon-vsock-agent`.

Wraps existing pack APIs — does **not** reimplement mkfs/docker:

| Step | API |
|------|-----|
| Stage agent (+ optional systemd unit) | `pack::stage_vsock_agent_into_tree` / bake flags |
| Tree → Ext4 | `bake_agent_then_pack` → `MkfsExt4` / `VirtMakeFs` |
| Docker → Ext4 | `bake_agent_then_pack` → `DockerToExt4` |
| Canned orchestration | `pack::build_canned_rootfs` / bin `eidolon-canned-rootfs` |

## Honesty

| Artifact | Status |
|----------|--------|
| Pack bake path (`bake_agent` / `bake_agent_then_pack`) | ✅ shipped (#109) |
| Canned pipeline (tree + bake + durable out + CLI) | ✅ shipped |
| Hermetic stub tree fixture + tests | ✅ shipped |
| Optional agent pin (`EIDOLON_VSOCK_AGENT_URL` + SHA-256) | ✅ scaffold (no default URL) |
| **Published GitHub release Ext4 disk asset** | ✅ pipeline + pin/fetch + env pin (`EIDOLON_ROOTFS_ASSET_URL`+`SHA256`) — default checkout pin **unpublished** until one-shot `gh release` (see `docs/guides/gh-ext4-release.md`) |

macOS CI cannot reliably produce a Linux musl guest binary or Ext4 image.
`CannedMode::TreeOnly` is the hermetic default; Ext4 pack fails loud without
`ROOTFS_PACK_INTEGRATION=1` + tools + agent.

## Output directory (durable — not `/tmp`)

| Source | Path |
|--------|------|
| Default | `target/canned-rootfs/` (workspace-relative) |
| Documented alt | `artifacts/rootfs/` |
| Override | `EIDOLON_CANNED_ROOTFS_OUT` / `--out` |

Paths under `/tmp` are rejected (`canned_guard_against_tmp`).

Layout after a tree-only build:

```text
target/canned-rootfs/
  rootfs-tree/
    usr/local/bin/eidolon-vsock-agent
    etc/systemd/system/eidolon-vsock-agent.service   # unless --no-systemd
    README.eidolon
  eidolon-canned-rootfs-manifest.json
```

Live Ext4 additionally writes `rootfs.img` + `eidolon-package-manifest.json`.

## Agent binary resolution

Fail-loud `EIDOLON_SANDBOX_VSOCK_AGENT_MISSING` when bake cannot find a binary:

1. `--agent` / `CannedRootfsRequest::agent_bin`
2. `EIDOLON_VSOCK_AGENT`
3. Cargo-target discovery (tests/dev)
4. Optional pin fetch when `--fetch-agent` / `allow_agent_fetch` **and** both
   `EIDOLON_VSOCK_AGENT_URL` + `EIDOLON_VSOCK_AGENT_SHA256` are set (curl +
   SHA-256 into `EIDOLON_VSOCK_AGENT_CACHE` or `~/.cache/eidolon/vsock-agent/`;
   requires feature `sandbox-rootfs-pack`)

Build the guest agent on Linux:

```bash
cargo build -p eidolon-sandbox --features sandbox-vsock-agent \
  --bin eidolon-vsock-agent --target x86_64-unknown-linux-musl --locked --release
export EIDOLON_VSOCK_AGENT=target/x86_64-unknown-linux-musl/release/eidolon-vsock-agent
```

## CLI

```bash
# Hermetic tree-only (stub fixture + fake or real agent)
cargo run -p eidolon-sandbox --bin eidolon-canned-rootfs --locked -- \
  --agent /path/to/eidolon-vsock-agent --mode tree_only

# Live Ext4 on Linux
ROOTFS_PACK_INTEGRATION=1 cargo run -p eidolon-sandbox \
  --features sandbox-rootfs-pack --bin eidolon-canned-rootfs --locked -- \
  --agent "$EIDOLON_VSOCK_AGENT" --mode pack_ext4

# Docker compose → Ext4
ROOTFS_PACK_INTEGRATION=1 cargo run -p eidolon-sandbox \
  --features sandbox-rootfs-pack --bin eidolon-canned-rootfs --locked -- \
  --agent "$EIDOLON_VSOCK_AGENT" --mode docker_to_ext4 --docker-ref alpine:3.19
```

## Rust API

```rust
use eidolon_sandbox::unikernel_pack::{
    build_canned_rootfs, CannedMode, CannedRootfsRequest,
};

let result = build_canned_rootfs(
    &CannedRootfsRequest::new()
        .with_agent_bin("target/release/eidolon-vsock-agent")
        .with_mode(CannedMode::TreeOnly)
        .with_systemd_unit(true),
)?;
assert!(result.bake.agent_guest_path.is_file());
```

## Tests

```bash
# Hermetic (fake agent + stub tree; no mkfs)
cargo test -p eidolon-sandbox --locked canned

# Live Ext4 when host tools present
ROOTFS_PACK_INTEGRATION=1 cargo test -p eidolon-sandbox --locked \
  --features sandbox-rootfs-pack --test rootfs_pack canned
```

## Related

- `docs/reference/rootfs-pack.md` — pack methods + env gates
- `docs/guides/vsock-guest-agent.md` — agent bake details
- `docs/guides/gh-ext4-release.md` — versioned GH Ext4 asset + pin/fetch
- `docs/EXTRACTION_PLAN.md` — T2 status

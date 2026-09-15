# Rootfs / guest image packaging

Hermetic packaging of existing artifacts is always-on. Live host-tool pipelines
require feature `sandbox-rootfs-pack` and `ROOTFS_PACK_INTEGRATION=1`.

## Pipeline steps (Docker → Ext4 disk for Firecracker)

```text
Docker image/container
        │
        ▼  PackMethod::DockerExport  (or step 1 of DockerToExt4)
   rootfs.tar  +  rootfs-tree/     ← RootfsFormat::Raw only
        │
        ▼  PackMethod::MkfsExt4  or  DiskImageBackend::VirtMakeFs
   rootfs.img                      ← RootfsFormat::Ext4
        │
        ▼  PackageManifest::to_rootfs_config()
   RootfsConfig / LaunchPlan       ← Firecracker boot
```

| Method | Input | Output artifact | Manifest format |
|--------|-------|-----------------|-----------------|
| `Hermetic` | existing image file | staged copy/link | as requested |
| `MkfsExt4` | directory tree (or pre-sized file) | `rootfs.img` | `ext4` |
| `VirtMakeFs` | directory tree | `rootfs.img` | `ext4` |
| `DockerExport` | image/container ref | `rootfs.tar` + `rootfs-tree/` | **`raw` only** |
| `DockerToExt4` | image/container ref | compose → `rootfs.img` | **`ext4`** |

### Compose helpers

- `unikernel_pack::live_pack` with `PackMethod::DockerToExt4` — full compose.
- `unikernel_pack::compose_docker_to_ext4` — same compose, explicit API.
- `unikernel_pack::pack_rootfs_tree_to_ext4` — existing `rootfs-tree/` → Ext4
  (no docker; uses `PackRequest::disk_backend`).
- `unikernel_pack::stage_vsock_agent_into_tree` / `bake_agent_then_pack` —
  stage `eidolon-vsock-agent` (+ optional systemd unit) into the tree before
  mkfs/virt-make-fs/DockerToExt4. Prefer `EIDOLON_VSOCK_AGENT`; missing agent
  when bake requested → `EIDOLON_SANDBOX_VSOCK_AGENT_MISSING` (never silent skip).
  See `docs/guides/vsock-guest-agent.md`.
- `unikernel_pack::build_canned_rootfs` / bin `eidolon-canned-rootfs` —
  durable canned tree (+ optional Ext4) under `EIDOLON_CANNED_ROOTFS_OUT` /
  `target/canned-rootfs/`. See `docs/guides/canned-rootfs.md`.
- `unikernel_pack::stage_release_asset` / bins `eidolon-release-rootfs` +
  `eidolon-fetch-rootfs` — versioned Ext4 + SHA-256 for `gh release upload`
  and UIA2-shaped pin/download. See `docs/guides/gh-ext4-release.md`.

Choose the disk tool via `PackRequest::with_disk_backend`:

- `DiskImageBackend::MkfsExt4` (default) — `mkfs.ext4 -F -d <tree> <img>`
- `DiskImageBackend::VirtMakeFs` — `virt-make-fs --type=ext4 --format=raw`

## Format honesty

`DockerExport` **never** claims `RootfsFormat::Ext4`. Requesting Ext4 with
`DockerExport` is a fail-loud `BadRequest`. Tar + extracted tree alone are not
a Firecracker disk — use `DockerToExt4` (or mkfs against an existing tree).

## Env / feature gates

| Gate | Effect |
|------|--------|
| Feature `sandbox-rootfs-pack` | SHA-256 checksums + live pipelines |
| `ROOTFS_PACK_INTEGRATION=1` | Allow host tool invocation |
| Missing | `EIDOLON_SANDBOX_ROOTFS_PACK_STUB` |

Tool overrides (explicit path; set-but-missing → not found, no PATH fallback):

- `EIDOLON_MKFS`
- `EIDOLON_VIRT_MAKE_FS`
- `EIDOLON_DOCKER`
- `EIDOLON_PACK_DOCKER_IMAGE` (live integration default image)
- `EIDOLON_VSOCK_AGENT` (host `eidolon-vsock-agent` binary for bake)
- `EIDOLON_CANNED_ROOTFS_OUT` (durable canned output; default `target/canned-rootfs/`)
- `EIDOLON_ROOTFS_RELEASE_OUT` (versioned release staging; default `target/rootfs-release/`)
- `EIDOLON_ROOTFS_RELEASE_CACHE` / `EIDOLON_ROOTFS_IMG` (pin download / BYO)
- `EIDOLON_VSOCK_AGENT_URL` + `EIDOLON_VSOCK_AGENT_SHA256` (optional pin fetch)
- `EIDOLON_VSOCK_AGENT_CACHE` (durable pin cache; default `~/.cache/eidolon/vsock-agent/`)

Error codes:

- `EIDOLON_SANDBOX_ROOTFS_PACK_TOOL_MISSING` — docker and/or mkfs/virt-make-fs absent
- `EIDOLON_SANDBOX_ROOTFS_PACK_IO` — command failure / I/O
- `EIDOLON_SANDBOX_VSOCK_AGENT_MISSING` — bake requested but agent binary absent

## Tests

```bash
cargo test -p eidolon-sandbox --locked
cargo test -p eidolon-sandbox --locked --features sandbox-rootfs-pack

# Optional live host tools:
ROOTFS_PACK_INTEGRATION=1 cargo test -p eidolon-sandbox --locked \
  --features sandbox-rootfs-pack --test rootfs_pack
```

Fake-tool unit tests live in `unikernel/pack/live.rs` (compose + honesty +
stage-agent-then-pack), `unikernel/pack/bake_agent.rs`, and
`unikernel/pack/canned.rs` (stub tree + durable out).
`unikernel/pack/release_asset.rs` (versioned GH asset + pin).

Canned CLI / recipe: `docs/guides/canned-rootfs.md`.
GH release recipe: `docs/guides/gh-ext4-release.md`.

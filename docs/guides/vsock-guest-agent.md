# In-guest vsock agent (`eidolon-vsock-agent`)

Guest-side AF_VSOCK listener that speaks the NDJSON v1 protocol in
[`unikernel-vsock-protocol.md`](../reference/unikernel-vsock-protocol.md).
Pairs with the host client in `eidolon_sandbox::unikernel::vsock` (`sandbox-vsock`).

## Honesty

| Surface | Behavior |
|---------|----------|
| Framing / handler | Always-on library (`unikernel::vsock_agent`) — unit-tested without real vsock |
| Binary | Feature `sandbox-vsock-agent` → `eidolon-vsock-agent` |
| Linux listen | `socket(AF_VSOCK)` + `bind` / `listen` / `accept` |
| macOS | Binary **compiles**; `serve` / runtime listen **fail loud** (`EIDOLON_SANDBOX_GUEST_IO_UNAVAILABLE`) |
| Commands | `validate_exec_cmd` then argv exec — **no shell** unless `--shell` / `EIDOLON_VSOCK_AGENT_ALLOW_SHELL=1` |
| Pack bake path | ✅ `unikernel::pack::bake_agent` + `PackRequest::bake_vsock_agent` / `bake_agent_then_pack` |
| Canned rootfs pipeline | ✅ `pack::canned` / `eidolon-canned-rootfs` (tree + bake; Ext4 gated) |
| Canned release rootfs image | ✅ staging + pin/fetch (`pack::release_asset`); default GH pin unpublished until one-shot publish |

Do **not** unarchive KDesktopVirt for this path.

## Build

Cross-compile or build on Linux for the guest rootfs:

```bash
# Host check (macOS): library + bin compile; listen fails loud at runtime
cargo build -p eidolon-sandbox --features sandbox-vsock-agent \
  --bin eidolon-vsock-agent --locked

# Guest (Linux) release binary
cargo build -p eidolon-sandbox --features sandbox-vsock-agent \
  --bin eidolon-vsock-agent --locked --release \
  --target x86_64-unknown-linux-musl   # or gnu, matching the rootfs
```

Binary path (after build):

```text
target/<triple>/release/eidolon-vsock-agent
# or without --target:
target/release/eidolon-vsock-agent
```

## Run (inside guest)

```bash
# Listen any CID (Firecracker guest style), port 5252
eidolon-vsock-agent --any-cid --port 5252

# Or env
export EIDOLON_VSOCK_PORT=5252
# optional: EIDOLON_VSOCK_CID=4294967295  # VMADDR_CID_ANY
eidolon-vsock-agent
```

Host then sets `EIDOLON_VSOCK_CID` / `EIDOLON_VSOCK_PORT` and
`UNIKERNEL_VSOCK_INTEGRATION=1` (see protocol doc).

## Bake into rootfs / pack pipeline

First-class pack APIs stage the agent into a rootfs tree **before**
`mkfs.ext4` / `virt-make-fs` / `DockerToExt4`:

| API | Role |
|-----|------|
| `pack::stage_vsock_agent_into_tree` | Stage-only (tree must exist) |
| `PackRequest::with_bake_vsock_agent(true)` | Bake during live pack |
| `pack::bake_agent_then_pack` | Force bake + `live_pack` (MkfsExt4 / VirtMakeFs / DockerToExt4) |

**Agent binary resolution** (fail-loud `EIDOLON_SANDBOX_VSOCK_AGENT_MISSING` — never silent skip):

1. `PackRequest::vsock_agent_bin` / `BakeAgentRequest::agent_bin`
2. **`EIDOLON_VSOCK_AGENT`** env (preferred production override; set-but-missing → no fallthrough)
3. Optional cargo-target discovery (`target/{debug,release}/eidolon-vsock-agent`) for tests

Guest install path: `/usr/local/bin/eidolon-vsock-agent`. Optional systemd unit via
`with_bake_systemd_unit(true)` → `etc/systemd/system/eidolon-vsock-agent.service`.

### Manual / shell equivalent

```bash
ROOTFS_TREE=/path/to/guest-tree
export EIDOLON_VSOCK_AGENT=target/x86_64-unknown-linux-musl/release/eidolon-vsock-agent
install -d "$ROOTFS_TREE/usr/local/bin"
install -m 0755 "$EIDOLON_VSOCK_AGENT" \
  "$ROOTFS_TREE/usr/local/bin/eidolon-vsock-agent"
```

### Example unit (also staged by bake when requested)

```ini
[Unit]
Description=Eidolon vsock NDJSON agent
After=local-fs.target

[Service]
ExecStart=/usr/local/bin/eidolon-vsock-agent --any-cid --port 5252
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

### Pack

```bash
# Hermetic unit tests (fake tools + bake):
cargo test -p eidolon-sandbox --locked --features sandbox-rootfs-pack

# Live mkfs / virt-make-fs / docker_to_ext4 (+ bake) when ROOTFS_PACK_INTEGRATION=1
ROOTFS_PACK_INTEGRATION=1 \
  EIDOLON_VSOCK_AGENT=target/release/eidolon-vsock-agent \
  cargo test -p eidolon-sandbox --locked --features sandbox-rootfs-pack
```

Rust sketch:

```rust
use eidolon_sandbox::unikernel::pack::{
    bake_agent_then_pack, PackMethod, PackRequest,
};
use eidolon_sandbox::unikernel::RootfsFormat;

let req = PackRequest::hermetic("/path/to/rootfs-tree", RootfsFormat::Ext4)
    .with_method(PackMethod::MkfsExt4)
    .with_staging("/path/to/staging", false)
    .with_bake_vsock_agent(true)
    .with_bake_systemd_unit(true);
// Or: bake_agent_then_pack(&req) — forces bake_vsock_agent
```

Firecracker vsock: configure the VM with a vsock device so the host can
`connect(guest_cid, 5252)` while the agent binds `VMADDR_CID_ANY:5252`.

**Honesty:** canned pipeline (`pack::canned` / `eidolon-canned-rootfs`) is
shipped for durable tree + bake; live Ext4 needs Linux tools + musl agent.
**Published GH Ext4 release** staging + pin/fetch are shipped
(`eidolon-release-rootfs` / `eidolon-fetch-rootfs`); default pin stays
unpublished until a manual one-shot `gh release upload`. See
`docs/guides/gh-ext4-release.md` and `docs/guides/canned-rootfs.md`.

## Tests

```bash
# Framing + handler (no real vsock) — always-on
cargo test -p eidolon-sandbox --locked vsock

# With agent feature (macOS: serve fail-loud; Linux: listen path linked)
cargo test -p eidolon-sandbox --locked --features sandbox-vsock-agent

# Pack + bake (fake tools under sandbox-rootfs-pack)
cargo test -p eidolon-sandbox --locked --features sandbox-rootfs-pack bake
```

## Related

- `docs/reference/unikernel-vsock-protocol.md` — wire format
- `docs/reference/rootfs-pack.md` — pack methods + env gates
- `docs/guides/canned-rootfs.md` — canned durable tree / Ext4 recipe + CLI
- `docs/guides/gh-ext4-release.md` — versioned GH Ext4 asset + pin/fetch
- `docs/EXTRACTION_PLAN.md` — T2 sandbox status
- Host feature: `sandbox-vsock`

# Unikernel vsock guest I/O protocol

Minimal framed request/response for guest exec over AF_VSOCK (Firecracker-style
CID/port). KISS: one NDJSON object per line. Do **not** unarchive KDesktopVirt
for this path.

## Status

| Layer | Status |
|-------|--------|
| NDJSON frame builders / parsers | ✅ always-on (`unikernel::vsock`) |
| Prefer vsock when CID+port env set | ✅ `GuestIoTransport::preferred` |
| Live `connect(AF_VSOCK)` | ✅ Linux + feature `sandbox-vsock` |
| macOS / feature-off | ✅ fail-loud `EIDOLON_SANDBOX_GUEST_IO_UNAVAILABLE` |
| Live destructive I/O env gate | ✅ `UNIKERNEL_VSOCK_INTEGRATION=1` |
| In-guest agent binary | ✅ `eidolon-vsock-agent` (`sandbox-vsock-agent`; see `docs/guides/vsock-guest-agent.md`) |
| Pack bake path (stage agent into tree before mkfs/DockerToExt4) | ✅ `unikernel::pack::bake_agent` / `bake_agent_then_pack` |
| Canned rootfs pipeline (durable tree + bake + CLI) | ✅ `unikernel::pack::canned` / `eidolon-canned-rootfs` |
| Published GH release Ext4 disk with agent | ✅ staging + pin/fetch (`pack::release_asset`); default pin unpublished until one-shot publish |

## Wire format (v1)

Encoding: UTF-8 JSON, one object per line, terminated by `\n`.

### Request (host → guest)

```json
{"v":1,"op":"exec","cmd":"uname -a","id":"optional"}
```

| Field | Required | Notes |
|-------|----------|-------|
| `v` | yes | Protocol version; must be `1` |
| `op` | yes | `"exec"` |
| `cmd` | yes | Already validated by host `validate_exec_cmd` |
| `id` | no | Correlation token echoed by agent when present |

### Response (guest → host)

```json
{"v":1,"op":"exec_result","ok":true,"stdout":"Linux\n","stderr":"","exit_code":0,"id":"optional"}
```

| Field | Required | Notes |
|-------|----------|-------|
| `v` | yes | Must equal request version (`1`) |
| `op` | yes | `"exec_result"` |
| `ok` | yes | `true` on success |
| `stdout` | no | Returned to `SandboxAutomator::exec` when `ok` |
| `stderr` | no | Used in fail-loud detail when `ok` is false |
| `exit_code` | no | Process exit status |
| `error` | no | Agent-level error string when `ok` is false |
| `id` | no | Echo of request id |

Host fail-loud code for transport/parse/agent errors:
`EIDOLON_SANDBOX_GUEST_IO_UNAVAILABLE`.

## Endpoint selection

```text
EIDOLON_VSOCK_CID=<u32>    # guest CID (typically >= 3)
EIDOLON_VSOCK_PORT=<u32>   # guest listen port (>= 1)
```

When **both** are set, `GuestIoTransport::preferred()` / `exec_cmd_on_guest`
use vsock; otherwise serial/stdio.

## Live gates

```bash
# Host build
cargo test -p eidolon-sandbox --locked --features sandbox-vsock,sandbox-unikernel

# Guest agent binary
cargo build -p eidolon-sandbox --locked --features sandbox-vsock-agent \
  --bin eidolon-vsock-agent

# Live AF_VSOCK round-trip (Linux + listening guest agent)
export UNIKERNEL_VSOCK_INTEGRATION=1
export EIDOLON_VSOCK_CID=3
export EIDOLON_VSOCK_PORT=5252
# plus boot env + live child as for serial exec
# guest: eidolon-vsock-agent --any-cid --port 5252
```

Serial remains available:

```bash
export UNIKERNEL_EXEC_INTEGRATION=1
# unset EIDOLON_VSOCK_CID / EIDOLON_VSOCK_PORT → serial preferred
```

## Platform notes

- **Linux + `sandbox-vsock`:** `socket(AF_VSOCK)` + `connect(sockaddr_vm)`.
- **macOS:** no AF_VSOCK — builders/parsers work; connect/exec vsock fail-loud.
- Guest must run an agent that speaks this NDJSON protocol on the configured
  port. Ship `eidolon-vsock-agent` (`sandbox-vsock-agent`) into the guest
  rootfs via `unikernel::pack::bake_agent` / `bake_agent_then_pack` (see
  `docs/guides/vsock-guest-agent.md`). Pack bake + canned pipeline shipped;
  GH Ext4 release staging + pin/fetch shipped (`docs/guides/gh-ext4-release.md`);
  default published asset pin may still be empty until one-shot `gh release`.

## Compose

`unikernel::exec::exec_on_guest` dispatches on `GuestIoTransport`:

- `SerialStdio` → child stdin/stdout (existing path)
- `Vsock { guest_cid, port }` → `unikernel::vsock::exec_via_vsock`

Clients (`UnikernelGuestClient`, `FirecrackerKvmClient`, `OpsNanoVmClient`)
call `exec_cmd_on_guest`, which uses preferred transport.

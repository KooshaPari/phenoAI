# bin/ — bundled executables

This directory ships pre-built executables that the forge3-bridge MCP server
relies on. The Python source for forge3_cli lives at `../forge3_cli.py`
(versioned alongside the server). The Rust daemon manager `forge3-ctl`
ships as both a binary and a Cargo source tree.

| Path | What | Rebuild |
|------|------|---------|
| `forge3-ctl` | Pre-built daemon manager (Rust, macOS) | `cd forge3-ctl-src && cargo build --release && cp target/release/forge3-ctl ../forge3-ctl` |
| `forge3-ctl-src/` | Cargo source for forge3-ctl (rebuildable) | `cargo build --release` |

## Why pre-bundle the binary?

The bridge server uses `forge3-ctl` to start / stop / status the long-lived
`forge3 ws` daemon. Without it, every MCP tool call falls back to spawning
`forge3 stdio` per request (correct but ~5× slower).

Most users install `forge3-ctl` from source. We ship a pre-built copy so the
`pip install` + `forge3-ctl install` one-step experience works on macOS
without requiring a Rust toolchain.

## Rebuilding

```bash
cd bin/forge3-ctl-src
cargo build --release
cp target/release/forge3-ctl ../forge3-ctl
```

## Cross-platform

The pre-built binary is for **macOS arm64/x86_64**. Linux/Windows users should
rebuild from `forge3-ctl-src/`. The source has zero platform-specific deps
beyond `std::process`, `nix`, and `ureq` — all pure Rust.

## Verified against

```
$ ./forge3-ctl --help
Synchronous CLI / install helper for the Forge Agent SDK (forge3).

Usage: forge3-ctl [OPTIONS] <COMMAND>

Commands:
  start    Spawn `forge3 ws` if not already running. Idempotent
  stop     Kill the locally-tracked forge3 ws daemon (started via `forge3-ctl start`)
  status   Print daemon + binary + transport sanity as JSON
  ping     One JSON-RPC round-trip: send `info`, print result
  methods  Print the rpc.discover method list
  call     Make a raw JSON-RPC call (uses stdio transport; no daemon needed)
  install  Install dotfile-style into ~/.claude/skills, ~/.codex/skills, ~/bin
```
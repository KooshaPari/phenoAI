//! In-guest AF_VSOCK NDJSON exec agent — speaks protocol v1 `exec` / `exec_result`.
//!
//! Build (Linux guest binary; macOS compiles but fail-loud at listen):
//! ```bash
//! cargo build -p eidolon-sandbox --features sandbox-vsock-agent \
//!   --bin eidolon-vsock-agent --locked --release
//! ```
//!
//! See `docs/guides/vsock-guest-agent.md` and
//! `docs/reference/unikernel-vsock-protocol.md`. Do not unarchive KDesktopVirt.

use eidolon_sandbox::unikernel::vsock_agent;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = vsock_agent::run_main(args);
    std::process::exit(code);
}

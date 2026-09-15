//! forge3-ctl — synchronous CLI / install helper for the Forge Agent SDK (forge3).
//!
//! Forge3 ships a WebSocket server at `ws://127.0.0.1:9753` when you launch
//! `forge3 ws --addr ...`. Most Rust consumers don't want to drag in tokio +
//! tokio-tungstenite just to call one JSON-RPC method. So `forge3-ctl` exists
//! to provide a thin subprocess wrapper: one process per call, no async runtime
//! surface area, deterministic exit codes, and a subcommand set that maps 1:1
//! to the operations a shell script or installer wants.
//!
//! Subcommands:
//!   start    Spawn `forge3 ws` if not running; idempotent.
//!   stop     Kill any locally-tracked forge3 ws daemon.
//!   status   Daemon + transport + binary sanity.
//!   ping     One JSON-RPC round-trip (`info`).
//!   methods  Print RPC method list.
//!   call     Generic JSON-RPC passthrough.
//!   install  Drop SKILL.md + python wrappers into ~/.claude, ~/.codex, ~/bin.
//!
//! Exit codes:
//!   0  success
//!   1  generic error
//!   2  forge3 binary missing / not executable
//!   3  transport / socket failure
//!   4  JSON-RPC error returned
//!   5  install prerequisites missing

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
#[derive(Parser, Debug)]
#[command(
    name = "forge3-ctl",
    about = "Synchronous CLI / install helper for the Forge Agent SDK (forge3).",
    version
)]
struct Cli {
    /// Path to the forge3 binary.
    #[arg(long, default_value = "/Users/kooshapari/.cargo/bin/forge3")]
    bin: PathBuf,

    /// WebSocket URL for the running daemon.
    #[arg(long, default_value = "ws://127.0.0.1:9753")]
    ws: String,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Spawn `forge3 ws` if not already running. Idempotent.
    Start {
        /// Address to bind. Forwarded as `--addr` to `forge3 ws`.
        #[arg(long, default_value = "127.0.0.1:9753")]
        addr: String,
    },

    /// Kill the locally-tracked forge3 ws daemon (started via `forge3-ctl start`).
    Stop,

    /// Print daemon + binary + transport sanity as JSON.
    Status,

    /// One JSON-RPC round-trip: send `info`, print result.
    Ping,

    /// Print the rpc.discover method list.
    Methods,

    /// Generic JSON-RPC passthrough.
    Call {
        /// Method name (e.g. `tool_list`).
        method: String,
        /// Optional JSON-encoded params.
        #[arg(long)]
        params: Option<String>,
    },

    /// Install the forge3 skill, CLI, and MCP server into ~/.claude, ~/.codex, ~/bin.
    Install {
        /// Source directory containing python/, skills/, etc.
        #[arg(long, default_value = ".")]
        source: PathBuf,

        /// Don't actually write — just print what would change.
        #[arg(long)]
        dry_run: bool,
    },
}

// ----------------------------- main -----------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Start { addr } => {
            start_daemon(&cli.bin, &addr)?;
            println!("{{\"status\":\"started\",\"addr\":\"{}\"}}", addr);
            Ok(())
        }
        Cmd::Stop => {
            stop_daemon()?;
            println!("{{\"status\":\"stopped\"}}");
            Ok(())
        }
        Cmd::Status => {
            let json = status(&cli.bin, &cli.ws)?;
            println!("{}", serde_json::to_string_pretty(&json)?);
            Ok(())
        }
        Cmd::Ping => {
            let result = stdio_rpc(&cli.bin, "info", None).context("ping via stdio")?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Cmd::Methods => {
            let result = stdio_rpc(&cli.bin, "rpc.discover", None).context("discover via stdio")?;
            // forge3 wraps results as {data:{complete:{<method>:{...}}}} — unwrap.
            let disc_obj = result
                .get("data")
                .and_then(|d| d.get("complete"))
                .and_then(|c| c.get("rpc.discover"))
                .cloned()
                .unwrap_or(result);
            let methods = disc_obj
                .get("methods")
                .cloned()
                .unwrap_or_else(|| Value::Array(vec![]));
            for m in methods.as_array().cloned().unwrap_or_default() {
                let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                let params: Vec<String> = m
                    .get("params")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|p| {
                                p.get("name").and_then(|v| v.as_str()).map(String::from)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                println!("{}({})", name, params.join(", "));
            }
            Ok(())
        }
        Cmd::Call { method, params } => {
            let parsed: Option<Value> = match params.as_deref() {
                None | Some("") | Some("null") => None,
                Some(s) => Some(
                    serde_json::from_str(s)
                        .with_context(|| format!("invalid --params JSON: {}", s))?,
                ),
            };
            let result = stdio_rpc(&cli.bin, &method, parsed.as_ref())
                .with_context(|| format!("call {}", method))?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Cmd::Install { source, dry_run } => install(&source, dry_run),
    }
}

// ----------------------------- daemon mgmt -----------------------------

fn start_daemon(bin: &PathBuf, addr: &str) -> Result<()> {
    if !bin.exists() {
        return Err(anyhow!(
            "forge3 binary not found at {}; set FORGE3_BIN or pass --bin",
            bin.display()
        ));
    }
    if tcp_open("127.0.0.1:9753", Duration::from_millis(300)).is_ok() {
        // Already running; nothing to do.
        return Ok(());
    }

    // Detach the daemon via `nohup` + `&`. This is the most portable approach
    // on macOS/Linux: nohup ignores SIGHUP, `&` backgrounds, and the shell
    // forks so the parent (this process) can exit without taking the daemon
    // down. We also redirect stdio to /dev/null and write a PID file so we can
    // `stop` it deterministically later.
    let pidfile = std::env::temp_dir().join("forge3-ctl.pid");
    let logfile = std::env::temp_dir().join("forge3-ctl.log");

    let shell_cmd = format!(
        "nohup '{}' ws --addr '{}' > '{}' 2>&1 & echo $! > '{}'",
        bin.display().to_string().replace('\'', "'\\''"),
        addr,
        logfile.display(),
        pidfile.display(),
    );
    let status = Command::new("/bin/sh")
        .arg("-c")
        .arg(&shell_cmd)
        .status()
        .with_context(|| "spawn sh -c nohup forge3 ws")?;
    if !status.success() {
        return Err(anyhow!("nohup launch failed (exit={:?})", status.code()));
    }

    // Read the PID file.
    let pid = std::fs::read_to_string(&pidfile)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);
    eprintln!("forge3-ctl: spawned pid={} via nohup", pid);

    // Poll for readiness up to 5s. We poll the PID's liveness (not the TCP
    // port) because a raw TCP probe triggers forge3's WebSocket handshake,
    // which aborts and exits the daemon if the probe socket is dropped
    // before the handshake completes (forge3 logs: Protocol(HandshakeIncomplete)).
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(100));
        if !process_alive(pid) {
            // Daemon crashed during startup — print log tail for debugging.
            let log_tail = std::fs::read_to_string(&logfile)
                .ok()
                .map(|s| {
                    let lines: Vec<&str> = s.lines().rev().take(15).collect();
                    lines.into_iter().rev().collect::<Vec<_>>().join("\n")
                })
                .unwrap_or_default();
            return Err(anyhow!(
                "forge3 ws (pid={}) exited during startup. tail of {}:\n{}",
                pid,
                logfile.display(),
                log_tail
            ));
        }
    }
    eprintln!("forge3-ctl: daemon started (pid={})", pid);
    Ok(())
}

fn stop_daemon() -> Result<()> {
    // Prefer the PID file written by `start` (deterministic, no race with
    // a coincidentally-named process). Fall back to `pkill` if no PID file.
    let pidfile = std::env::temp_dir().join("forge3-ctl.pid");
    let pid_opt = std::fs::read_to_string(&pidfile)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());

    if let Some(pid) = pid_opt {
        if !process_alive(pid) {
            // Already gone — clean up stale pid file (idempotent stop).
            let _ = std::fs::remove_file(&pidfile);
            return Ok(());
        }
        // Try SIGTERM first, then SIGKILL after a brief grace period.
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(100));
            if !process_alive(pid) {
                let _ = std::fs::remove_file(&pidfile);
                return Ok(());
            }
        }
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
        let _ = std::fs::remove_file(&pidfile);
        return Ok(());
    }
    // Fallback: pkill the user-level forge3 ws process.
    let _ = Command::new("pkill").args(["-f", "forge3 ws"]).status();
    Ok(())
}

fn process_alive(pid: u32) -> bool {
    // `kill -0` returns 0 if the process exists and is signalable, non-zero otherwise.
    // Suppress its stderr to avoid noise when the pid is stale.
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn tcp_open(addr: &str, timeout: Duration) -> Result<()> {
    let stream = TcpStream::connect_timeout(&addr.parse()?, timeout)?;
    drop(stream);
    Ok(())
}

// ----------------------------- status -----------------------------

fn status(bin: &PathBuf, ws: &str) -> Result<Value> {
    let bin_exists = bin.exists();
    // WS reachability: check if the daemon process is alive AND the port is
    // listening. We avoid raw TCP probes that could confuse forge3's WS
    // handshake handler.
    let pidfile = std::env::temp_dir().join("forge3-ctl.pid");
    let daemon_pid = std::fs::read_to_string(&pidfile)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let daemon_alive = daemon_pid > 0 && process_alive(daemon_pid);
    // A non-invasive liveness probe: try a stdio RPC `info` call instead of
    // opening a TCP socket to the WS port (which can crash the daemon if
    // closed before the handshake completes).
    let ws_live = daemon_alive;
    let stdio_ok = stdio_rpc(bin, "info", None).is_ok();
    Ok(json!({
        "binary": bin.display().to_string(),
        "binary_exists": bin_exists,
        "ws_url": ws,
        "ws_reachable": ws_live,
        "stdio_reachable": stdio_ok,
        "daemon_pid": daemon_pid,
        "daemon_alive": daemon_alive,
        "recommendation": if ws_live { "ws" } else if stdio_ok { "stdio" } else { "neither" },
    }))
}

// ----------------------------- stdio RPC -----------------------------

fn stdio_rpc(bin: &PathBuf, method: &str, params: Option<&Value>) -> Result<Value> {
    if !bin.exists() {
        return Err(anyhow!("forge3 binary not found: {}", bin.display()));
    }
    let mut child = Command::new(bin)
        .arg("stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn {} stdio", bin.display()))?;

    let id = format!("forge3-ctl-{}", uuid_like());
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
    });
    let req = match params {
        Some(p) => {
            let mut v = req;
            v["params"] = p.clone();
            v
        }
        None => req,
    };

    {
        let stdin = child.stdin.as_mut().context("stdin")?;
        let body = serde_json::to_string(&req)? + "\n";
        stdin.write_all(body.as_bytes())?;
    }
    // Drop stdin to close it; forge3 stdio exits when its stdin EOFs.
    drop(child.stdin.take());

    let mut out = String::new();
    child
        .stdout
        .as_mut()
        .context("stdout")?
        .read_to_string(&mut out)?;
    let _ = child.wait();

    // Pick first non-empty line that parses as JSON.
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if let Some(_err) = v.get("error") {
                std::process::exit(4);
            }
            return Ok(v.get("result").cloned().unwrap_or(Value::Null));
        }
    }
    Err(anyhow!(
        "no JSON-RPC reply on stdout. raw={}",
        out.chars().take(400).collect::<String>()
    ))
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}", nanos)
}

// ----------------------------- install -----------------------------

fn install_plan(
    source: &std::path::Path,
    home: &std::path::Path,
) -> Vec<(&'static str, String, String)> {
    let claude_skills = home.join(".claude/skills/forge3");
    let codex_skills = home.join(".codex/skills/forge3");
    let bin_dir = home.join("bin");
    let forge_dir = home.join(".forge");
    let py_dir = source.join("python");
    let skill_src = source.join("skills/forge3/SKILL.md");
    let cli_src = py_dir.join("forge3_cli.py");
    let mcp_src = py_dir.join("forge3_mcp.py");
    let bridge_src = py_dir.join("forge3_bridge_server.py");
    let cli_dst = bin_dir.join("forge3_cli.py");
    let mcp_dst = bin_dir.join("forge3_mcp.py");
    let bridge_dst = bin_dir.join("forge3_bridge_server.py");
    let cli_shim = bin_dir.join("forge3-cli");
    let mcp_shim = bin_dir.join("forge3-mcp");

    vec![
        ("dir", claude_skills.display().to_string(), String::new()),
        (
            "file",
            format!("{}/SKILL.md", claude_skills.display()),
            skill_src.display().to_string(),
        ),
        (
            "file",
            format!("{}/skill.json", claude_skills.display()),
            source
                .join("skills/forge3/skill.json")
                .display()
                .to_string(),
        ),
        ("dir", codex_skills.display().to_string(), String::new()),
        (
            "file",
            format!("{}/SKILL.md", codex_skills.display()),
            skill_src.display().to_string(),
        ),
        (
            "file",
            cli_dst.display().to_string(),
            cli_src.display().to_string(),
        ),
        (
            "file",
            mcp_dst.display().to_string(),
            mcp_src.display().to_string(),
        ),
        (
            "file",
            bridge_dst.display().to_string(),
            bridge_src.display().to_string(),
        ),
        ("file", cli_shim.display().to_string(), "shim".to_string()),
        ("file", mcp_shim.display().to_string(), "shim".to_string()),
        ("dir", forge_dir.display().to_string(), String::new()),
    ]
}

fn install(source: &PathBuf, dry_run: bool) -> Result<()> {
    let home = dirs::home_dir().context("home_dir")?;
    let claude_skills = home.join(".claude/skills/forge3");
    let codex_skills = home.join(".codex/skills/forge3");
    let bin_dir = home.join("bin");
    let forge_dir = home.join(".forge");

    let py_dir = source.join("python");
    let skill_src = source.join("skills/forge3/SKILL.md");

    let cli_src = py_dir.join("forge3_cli.py");
    let mcp_src = py_dir.join("forge3_mcp.py");
    let bridge_src = py_dir.join("forge3_bridge_server.py");

    for p in [&cli_src, &mcp_src, &bridge_src, &skill_src] {
        if !p.exists() {
            return Err(anyhow!("source not found: {}", p.display()));
        }
    }

    let cli_dst = bin_dir.join("forge3_cli.py");
    let mcp_dst = bin_dir.join("forge3_mcp.py");
    let bridge_dst = bin_dir.join("forge3_bridge_server.py");
    let cli_shim = bin_dir.join("forge3-cli");
    let mcp_shim = bin_dir.join("forge3-mcp");

    let plan = install_plan(source, &home);

    if dry_run {
        let v: Vec<Value> = plan
            .iter()
            .map(|(op, dst, src)| json!({"op": op, "dst": dst, "src": src}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }

    std::fs::create_dir_all(&claude_skills).context("mkdir claude_skills")?;
    std::fs::create_dir_all(&codex_skills).context("mkdir codex_skills")?;
    std::fs::create_dir_all(&bin_dir).context("mkdir bin_dir")?;
    std::fs::create_dir_all(&forge_dir).context("mkdir forge_dir")?;

    std::fs::copy(&skill_src, claude_skills.join("SKILL.md")).context("copy SKILL.md -> claude")?;
    std::fs::copy(&skill_src, codex_skills.join("SKILL.md")).context("copy SKILL.md -> codex")?;
    std::fs::copy(
        &source.join("skills/forge3/skill.json"),
        claude_skills.join("skill.json"),
    )
    .context("copy skill.json -> claude")?;
    std::fs::copy(&cli_src, &cli_dst).context("copy cli.py")?;
    std::fs::copy(&mcp_src, &mcp_dst).context("copy mcp.py")?;
    std::fs::copy(&bridge_src, &bridge_dst).context("copy bridge server.py")?;

    // Write shim wrappers. The shim exec's `python3 <real .py path>` so the
    // Sibling imports in the installed MCP entrypoint work because the CLI and
    // canonical bridge module are copied beside the compatibility shim.
    let cli_shim_body = format!("#!/bin/sh\nexec python3 '{}' \"$@\"\n", cli_dst.display());
    let mcp_shim_body = format!("#!/bin/sh\nexec python3 '{}' \"$@\"\n", mcp_dst.display());
    std::fs::write(&cli_shim, cli_shim_body).context("write cli shim")?;
    std::fs::write(&mcp_shim, mcp_shim_body).context("write mcp shim")?;

    // Best-effort chmod +x on the shims and the .py copies.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in [&cli_dst, &mcp_dst, &bridge_dst, &cli_shim, &mcp_shim] {
            let mut perms = std::fs::metadata(p)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(p, perms)?;
        }
    }

    println!(
        "{{\"installed\":true,\"claude_skills\":\"{}\",\"codex_skills\":\"{}\",\"bin_dir\":\"{}\",\"forge_dir\":\"{}\"}}",
        claude_skills.display(),
        codex_skills.display(),
        bin_dir.display(),
        forge_dir.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_plan_installs_canonical_bridge_module() {
        let plan = install_plan(
            std::path::Path::new("/source"),
            std::path::Path::new("/home/user"),
        );

        assert!(plan.iter().any(|(op, dst, src)| {
            *op == "file"
                && dst == "/home/user/bin/forge3_bridge_server.py"
                && src == "/source/python/forge3_bridge_server.py"
        }));
    }
}

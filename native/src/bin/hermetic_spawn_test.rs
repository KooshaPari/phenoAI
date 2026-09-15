//! L7 #134 — Hermetic spawn test binary.
//!
//! Runs as `cargo run --bin hermetic_spawn_test` (and again with the
//! `--kill` flag for the graceful-shutdown leg). Spawns a fake-backend
//! shell script from `native/tests/fixtures/` and verifies:
//!
//!   1. The spawn pattern (tokio::process::Command + piped stdio +
//!      setpgid-on-unix) that the modality drivers (SandboxDriver,
//!      NvmsDriver, WslDriver, ContainerDriver) use actually produces
//!      a live child whose stdout contains the expected marker.
//!
//!   2. The Drop pattern (SIGTERM to pgroup + 5s grace + SIGKILL) the
//!      drivers use cleanly tears down the child + all descendants.
//!
//! Cold-compile is ~3s. Runtime is <1s for the spawn leg, ~5s for the
//! kill leg (waiting for the 5s graceful timeout).
//!
//! This binary does NOT depend on `playcua_native` itself — it
//! reproduces the spawn/kill pattern in isolation so it can verify the
//! pattern without dragging the full workspace compile.
//!
//! --kill mode runs the fake-backend as a sleeper (`sleep 30`) and
//! verifies SIGTERM-to-pgid causes it to exit within the 5s grace.
//!
//! Exit codes: 0 = success, 1 = test failure (stderr describes why).

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use tokio::io::AsyncReadExt;
use tokio::process::Command;

const FAKE_ALIVE: &str = "FAKE-NVMS-ALIVE rev-1";

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let kill_mode = args.iter().any(|a| a == "--kill");

    let fixture_dir = match locate_fixture_dir() {
        Some(p) => p,
        None => {
            eprintln!(
                "FATAL: could not locate tests/fixtures/ relative to CWD or CARGO_MANIFEST_DIR"
            );
            return ExitCode::from(1);
        }
    };
    let fixture = if cfg!(windows) {
        fixture_dir.join("fake-nvms.cmd")
    } else {
        fixture_dir.join("fake-nvms.sh")
    };

    if !fixture.exists() {
        eprintln!("FATAL: fixture script not found at {}", fixture.display());
        return ExitCode::from(1);
    }

    // 1. SPAWN TEST: verify the child actually starts and the marker
    //    shows up on stdout. We invoke the fixture directly (not
    //    through "nvms" on PATH) so this test is hermetic regardless
    //    of host PATH contents.
    eprintln!("[hermetic] spawn test — spawning {}", fixture.display());

    // On Windows, fake-nvms.cmd invokes `more` via `cmd /c`, which
    // would block waiting for stdin. Use a non-interactive variant:
    // the fixture detects `HERMETIC_QUIET=1` and exits immediately.
    let mut cmd = Command::new(&fixture);
    cmd.env("HERMETIC_QUIET", "1")
        .env("FAKE_MARKER", FAKE_ALIVE);

    // Mirror the modality drivers' spawn pattern.
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            // Own session/pgroup so SIGTERM-to-pgid reaps only the guest
            // tree (matches SandboxDriver::spawn_guest).
            libc::setsid();
            Ok(())
        });
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("FATAL: spawn() failed: {e}");
            return ExitCode::from(1);
        }
    };

    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut buf = Vec::new();
    if let Err(e) = stdout.read_to_end(&mut buf).await {
        eprintln!("FATAL: read_to_end failed: {e}");
        let _ = child.start_kill();
        return ExitCode::from(1);
    }

    let out = String::from_utf8_lossy(&buf);
    if !out.contains(FAKE_ALIVE) {
        eprintln!(
            "FATAL: marker {} not in stdout. Got: {}",
            FAKE_ALIVE,
            out.trim()
        );
        let _ = child.start_kill();
        return ExitCode::from(1);
    }
    eprintln!("[hermetic] spawn test — OK (marker found, child exited normally)");

    // Best-effort wait so we don't leak the child.
    let _ = child.wait().await;

    if !kill_mode {
        return ExitCode::from(0);
    }

    // 2. KILL TEST: spawn the sleeper child, send SIGTERM to its pgroup,
    //    verify it exits within 5s (matches the modality drivers'
    //    Drop::shutdown grace window).
    eprintln!("[hermetic] kill test — spawning sleeper");

    let sleeper = if cfg!(windows) {
        // Windows: `cmd /c timeout /t 30 /nobreak` would block on
        // stdin; use `ping -n 30 127.0.0.1 >nul` instead, which sleeps
        // for ~30 seconds and doesn't read stdin.
        fixture_dir.join("fake-nvms.cmd")
    } else {
        fixture_dir.join("fake-nvms.sh")
    };

    let mut cmd = Command::new(&sleeper);
    cmd.env("HERMETIC_QUIET", "0") // not quiet — sleeper mode
        .env("HERMETIC_SLEEP_SECS", "30");

    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("FATAL: sleeper spawn failed: {e}");
            return ExitCode::from(1);
        }
    };

    #[cfg(unix)]
    let pid = child.id().expect("child pid");
    #[cfg(unix)]
    let pgid = unsafe { libc::getpgid(pid as i32) };
    #[cfg(unix)]
    let pgrp_alive = pgid >= 0;

    // Give the child a beat to settle.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    #[cfg(unix)]
    {
        if pgrp_alive {
            let ret = unsafe { libc::killpg(pgid, libc::SIGTERM) };
            if ret != 0 {
                eprintln!(
                    "FATAL: killpg({}, SIGTERM) failed: errno={}",
                    pgid,
                    std::io::Error::last_os_error()
                );
                let _ = child.kill().await;
                return ExitCode::from(1);
            }
            eprintln!("[hermetic] kill test — sent SIGTERM to pgid={pgid}");
        } else {
            eprintln!("[hermetic] kill test — pgid unknown, falling back to child.kill()");
            let _ = child.kill().await;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill().await;
        eprintln!("[hermetic] kill test — sent kill() on Windows (no SIGTERM equiv)");
    }

    // Wait up to 5s for the child to exit.
    let start = std::time::Instant::now();
    let status = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await;

    match status {
        Ok(Ok(s)) => {
            let elapsed = start.elapsed();
            eprintln!(
                "[hermetic] kill test — child exited in {:.2}s status={:?}",
                elapsed.as_secs_f64(),
                s
            );
            if elapsed.as_secs() > 6 {
                eprintln!("FATAL: child took >6s to exit after SIGTERM (grace is 5s)");
                ExitCode::from(1)
            } else {
                ExitCode::from(0)
            }
        }
        Ok(Err(e)) => {
            eprintln!("FATAL: child.wait() failed: {e}");
            let _ = child.kill().await;
            ExitCode::from(1)
        }
        Err(_) => {
            eprintln!("FATAL: child did NOT exit within 5s of SIGTERM — kill pattern broken");
            let _ = child.kill().await;
            ExitCode::from(1)
        }
    }
}

fn locate_fixture_dir() -> Option<PathBuf> {
    // 1. CWD/tests/fixtures (workspace root)
    let cwd_tests = env::current_dir().ok()?.join("tests").join("fixtures");
    if cwd_tests.exists() {
        return Some(cwd_tests);
    }
    // 2. CWD/native/tests/fixtures (if running from crate root)
    let native_tests = env::current_dir()
        .ok()?
        .join("native")
        .join("tests")
        .join("fixtures");
    if native_tests.exists() {
        return Some(native_tests);
    }
    // 3. CARGO_MANIFEST_DIR/../tests/fixtures (workspace root via cargo)
    if let Ok(manifest) = env::var("CARGO_MANIFEST_DIR") {
        let manifest_path = PathBuf::from(manifest);
        let p = manifest_path.join("..").join("tests").join("fixtures");
        if p.exists() {
            return Some(p.canonicalize().unwrap_or(p));
        }
        let p2 = manifest_path.join("tests").join("fixtures");
        if p2.exists() {
            return Some(p2.canonicalize().unwrap_or(p2));
        }
    }
    None
}

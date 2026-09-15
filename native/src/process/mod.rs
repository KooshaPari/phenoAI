//! Process management: launch, kill, status.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

/// Shared map of launched child PIDs → Child handles for status tracking.
/// We store the exit status once the process exits.
static CHILD_MAP: std::sync::OnceLock<Arc<Mutex<HashMap<u32, ChildState>>>> =
    std::sync::OnceLock::new();

fn child_map() -> &'static Arc<Mutex<HashMap<u32, ChildState>>> {
    CHILD_MAP.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

#[derive(Debug)]
enum ChildState {
    Running,
    Exited(i32),
}

// ---------------------------------------------------------------------------
// RPC handlers
// ---------------------------------------------------------------------------

pub async fn launch_rpc(params: Value) -> Result<Value> {
    #[derive(Deserialize)]
    struct LaunchParams {
        path: String,
        args: Option<Vec<String>>,
        cwd: Option<String>,
    }
    let p: LaunchParams = serde_json::from_value(params)?;
    let pid = launch(&p.path, p.args.as_deref().unwrap_or(&[]), p.cwd.as_deref()).await?;
    Ok(json!({ "pid": pid }))
}

pub async fn kill_rpc(params: Value) -> Result<Value> {
    #[derive(Deserialize)]
    struct KillParams { pid: u32 }
    let p: KillParams = serde_json::from_value(params)?;
    kill(p.pid).await?;
    Ok(json!({ "ok": true }))
}

pub async fn status_rpc(params: Value) -> Result<Value> {
    #[derive(Deserialize)]
    struct StatusParams { pid: u32 }
    let p: StatusParams = serde_json::from_value(params)?;
    let st = status(p.pid).await?;
    Ok(json!({
        "running": st.running,
        "exit_code": st.exit_code,
    }))
}

// ---------------------------------------------------------------------------
// Core implementations
// ---------------------------------------------------------------------------

/// Launch a process non-blocking. Returns the PID.
pub async fn launch(path: &str, args: &[String], cwd: Option<&str>) -> Result<u32> {
    let path = path.to_string();
    let args = args.to_vec();
    let cwd = cwd.map(|s| s.to_string());

    let pid = tokio::task::spawn_blocking(move || -> Result<u32> {
        let mut cmd = std::process::Command::new(&path);
        cmd.args(&args);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        if let Some(ref dir) = cwd {
            cmd.current_dir(dir);
        }
        let child = cmd.spawn().with_context(|| format!("Failed to spawn: {}", path))?;
        let pid = child.id();
        debug!("Spawned process pid={}", pid);
        child_map().lock().unwrap().insert(pid, ChildState::Running);
        // Note: child handle is dropped here. We track by PID only.
        Ok(pid)
    })
    .await
    .context("spawn_blocking panicked")??;

    info!("process.launch: pid={}", pid);
    Ok(pid)
}

/// Kill a process by PID.
pub async fn kill(pid: u32) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
        }
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::{
                Foundation::CloseHandle,
                System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE},
            };
            unsafe {
                let handle = OpenProcess(PROCESS_TERMINATE, false, pid)
                    .context("OpenProcess failed")?;
                TerminateProcess(handle, 1).context("TerminateProcess failed")?;
                let _ = CloseHandle(handle);
            }
        }
        child_map()
            .lock()
            .unwrap()
            .insert(pid, ChildState::Exited(-1));
        info!("process.kill: pid={}", pid);
        Ok(())
    })
    .await
    .context("spawn_blocking panicked")?
}

pub struct ProcessStatus {
    pub running: bool,
    pub exit_code: Option<i32>,
}

/// Check if a process is still running.
pub async fn status(pid: u32) -> Result<ProcessStatus> {
    tokio::task::spawn_blocking(move || -> Result<ProcessStatus> {
        let running = is_running(pid);
        if running {
            Ok(ProcessStatus { running: true, exit_code: None })
        } else {
            let code = child_map()
                .lock()
                .unwrap()
                .get(&pid)
                .and_then(|s| match s {
                    ChildState::Exited(c) => Some(*c),
                    ChildState::Running => None,
                });
            Ok(ProcessStatus { running: false, exit_code: code })
        }
    })
    .await
    .context("spawn_blocking panicked")?
}

/// Platform-specific alive check using OS APIs (no SIGKILL).
fn is_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // kill(pid, 0) returns 0 if process exists.
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::{
            Foundation::CloseHandle,
            System::Threading::{
                GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        };
        unsafe {
            let Ok(handle) =
                OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            else {
                return false;
            };
            let mut code: u32 = 0;
            let ok = GetExitCodeProcess(handle, &mut code);
            let _ = CloseHandle(handle);
            // STILL_ACTIVE = 259
            ok.is_ok() && code == 259
        }
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    false
}

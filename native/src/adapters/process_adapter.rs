//! NativeProcessAdapter — cross-platform process management using std::process.
//! Implements ProcessPort.

use crate::domain::process::{ProcessError, ProcessHandle, ProcessStatus};
use crate::ports::ProcessPort;
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use tracing::{debug, info, instrument};

#[derive(Debug)]
enum ChildState {
    Running,
    Exited(i32),
}

static CHILD_MAP: OnceLock<Arc<Mutex<HashMap<u32, ChildState>>>> = OnceLock::new();

fn child_map() -> &'static Arc<Mutex<HashMap<u32, ChildState>>> {
    CHILD_MAP.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

pub struct NativeProcessAdapter;

impl NativeProcessAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NativeProcessAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProcessPort for NativeProcessAdapter {
    #[instrument(name = "process.launch", skip(self), fields(path = %handle.path))]
    async fn launch(&self, handle: ProcessHandle) -> Result<u32, ProcessError> {
        let path = handle.path.clone();
        let args = handle.args.clone();
        let cwd = handle.cwd.clone();

        let pid = tokio::task::spawn_blocking(move || -> Result<u32, ProcessError> {
            let mut cmd = std::process::Command::new(&path);
            cmd.args(&args);
            cmd.stdin(Stdio::null());
            cmd.stdout(Stdio::null());
            cmd.stderr(Stdio::null());
            if let Some(ref dir) = cwd {
                cmd.current_dir(dir);
            }
            let child = cmd
                .spawn()
                .map_err(|e| ProcessError::LaunchFailed(format!("{}: {}", path, e)))?;
            let pid = child.id();
            debug!("Spawned process pid={}", pid);
            child_map().lock().unwrap().insert(pid, ChildState::Running);
            Ok(pid)
        })
        .await
        .map_err(|e| ProcessError::LaunchFailed(format!("spawn_blocking panic: {e}")))??;

        info!("process.launch: pid={}", pid);
        Ok(pid)
    }

    #[instrument(name = "process.kill", skip(self), fields(pid = pid))]
    async fn kill(&self, pid: u32) -> Result<(), ProcessError> {
        tokio::task::spawn_blocking(move || -> Result<(), ProcessError> {
            kill_platform(pid)?;
            child_map()
                .lock()
                .unwrap()
                .insert(pid, ChildState::Exited(-1));
            info!("process.kill: pid={}", pid);
            Ok(())
        })
        .await
        .map_err(|e| ProcessError::KillFailed(format!("spawn_blocking panic: {e}")))?
    }

    #[instrument(name = "process.status", skip(self), fields(pid = pid))]
    async fn status(&self, pid: u32) -> Result<ProcessStatus, ProcessError> {
        tokio::task::spawn_blocking(move || -> Result<ProcessStatus, ProcessError> {
            let running = is_running_platform(pid);
            if running {
                Ok(ProcessStatus {
                    running: true,
                    exit_code: None,
                })
            } else {
                let code = child_map().lock().unwrap().get(&pid).and_then(|s| match s {
                    ChildState::Exited(c) => Some(*c),
                    ChildState::Running => None,
                });
                Ok(ProcessStatus {
                    running: false,
                    exit_code: code,
                })
            }
        })
        .await
        .map_err(|e| ProcessError::StatusFailed(format!("spawn_blocking panic: {e}")))?
    }
}

#[cfg(unix)]
fn kill_platform(pid: u32) -> Result<(), ProcessError> {
    let ret = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if ret != 0 {
        Err(ProcessError::KillFailed(format!(
            "kill({}) failed: errno={}",
            pid, ret
        )))
    } else {
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn kill_platform(pid: u32) -> Result<(), ProcessError> {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE},
    };
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, false, pid)
            .map_err(|e| ProcessError::KillFailed(format!("OpenProcess: {e}")))?;
        TerminateProcess(handle, 1)
            .map_err(|e| ProcessError::KillFailed(format!("TerminateProcess: {e}")))?;
        let _ = CloseHandle(handle);
    }
    Ok(())
}

#[cfg(not(any(unix, target_os = "windows")))]
fn kill_platform(_pid: u32) -> Result<(), ProcessError> {
    Err(ProcessError::KillFailed(
        "process.kill not supported on this platform".to_string(),
    ))
}

#[cfg(unix)]
fn is_running_platform(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(target_os = "windows")]
fn is_running_platform(pid: u32) -> bool {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut code: u32 = 0;
        let ok = GetExitCodeProcess(handle, &mut code);
        let _ = CloseHandle(handle);
        ok.is_ok() && code == 259 // STILL_ACTIVE
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
fn is_running_platform(_pid: u32) -> bool {
    false
}

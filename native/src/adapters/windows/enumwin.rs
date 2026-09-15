//! EnumWindowsAdapter — Windows window enumeration via Win32 EnumWindows.
//! Implements WindowPort using GetWindowText, GetWindowThreadProcessId,
//! GetWindowRect, IsWindowVisible, and SetForegroundWindow.

use crate::domain::window::{WindowError, WindowFilter, WindowInfo};
use crate::ports::WindowPort;
use async_trait::async_trait;
use tracing::instrument;

pub struct EnumWindowsAdapter;

impl EnumWindowsAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EnumWindowsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WindowPort for EnumWindowsAdapter {
    #[instrument(name = "enumwin.list_windows", skip(self))]
    async fn list_windows(&self) -> Result<Vec<WindowInfo>, WindowError> {
        tokio::task::spawn_blocking(enum_windows_sync)
            .await
            .map_err(|e| WindowError::Failed(format!("spawn_blocking panic: {e}")))?
    }

    #[instrument(name = "enumwin.find_window", skip(self), fields(title = ?filter.title, pid = ?filter.pid))]
    async fn find_window(&self, filter: WindowFilter) -> Result<Option<WindowInfo>, WindowError> {
        let all = self.list_windows().await?;
        let found = all.into_iter().find(|w| {
            let title_match = filter
                .title
                .as_ref()
                .map_or(true, |t| w.title.to_lowercase().contains(&t.to_lowercase()));
            let pid_match = filter.pid.map_or(true, |p| w.pid == p);
            title_match && pid_match
        });
        Ok(found)
    }

    #[instrument(name = "enumwin.focus_window", skip(self), fields(hwnd = hwnd))]
    async fn focus_window(&self, hwnd: usize) -> Result<(), WindowError> {
        tokio::task::spawn_blocking(move || set_foreground_sync(hwnd))
            .await
            .map_err(|e| WindowError::Failed(format!("spawn_blocking panic: {e}")))?
    }
}

// ---------------------------------------------------------------------------
// Blocking Win32 implementations
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn enum_windows_sync() -> Result<Vec<WindowInfo>, WindowError> {
    use std::sync::{Arc, Mutex};
    use windows::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        },
    };

    let results: Arc<Mutex<Vec<WindowInfo>>> = Arc::new(Mutex::new(Vec::new()));
    let results_clone = results.clone();

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
        let results_ptr = lparam.0 as *const Arc<Mutex<Vec<WindowInfo>>>;
        let results = unsafe { &*results_ptr };

        let mut title_buf = [0u16; 512];
        let title_len = unsafe { GetWindowTextW(hwnd, &mut title_buf) };
        if title_len == 0 {
            return windows::core::BOOL(1);
        }
        let title = String::from_utf16_lossy(&title_buf[..title_len as usize]);

        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };

        let mut rect = windows::Win32::Foundation::RECT::default();
        let _ = unsafe { GetWindowRect(hwnd, &mut rect) };

        let visible = unsafe { IsWindowVisible(hwnd).as_bool() };

        let info = WindowInfo {
            hwnd: hwnd.0 as usize,
            title,
            pid,
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
            visible,
        };

        if let Ok(mut v) = results.lock() {
            v.push(info);
        }
        windows::core::BOOL(1)
    }

    let ptr = &results_clone as *const Arc<Mutex<Vec<WindowInfo>>>;
    unsafe {
        EnumWindows(
            Some(enum_proc as unsafe extern "system" fn(HWND, LPARAM) -> windows::core::BOOL),
            LPARAM(ptr as isize),
        )
        .map_err(|e| WindowError::EnumerationFailed(e.to_string()))?;
    }

    let vec = match Arc::try_unwrap(results) {
        Ok(mutex) => mutex.into_inner().unwrap_or_default(),
        Err(arc) => arc.lock().map(|g| g.clone()).unwrap_or_default(),
    };
    Ok(vec)
}

#[cfg(not(target_os = "windows"))]
fn enum_windows_sync() -> Result<Vec<WindowInfo>, WindowError> {
    Err(WindowError::Failed(
        "EnumWindows is only available on Windows".to_string(),
    ))
}

#[cfg(target_os = "windows")]
fn set_foreground_sync(hwnd: usize) -> Result<(), WindowError> {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::SetForegroundWindow};
    unsafe {
        let _ = SetForegroundWindow(HWND(hwnd as *mut core::ffi::c_void));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn set_foreground_sync(_hwnd: usize) -> Result<(), WindowError> {
    Err(WindowError::Failed(
        "SetForegroundWindow is only available on Windows".to_string(),
    ))
}

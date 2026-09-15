//! Windows window enumeration using Win32 EnumWindows + GetWindowText.

use super::WindowInfo;
use anyhow::{Context, Result};
use tracing::warn;

pub async fn list_windows() -> Result<Vec<WindowInfo>> {
    tokio::task::spawn_blocking(enum_windows_sync)
        .await
        .context("spawn_blocking panicked")?
}

pub async fn focus_window(hwnd: i64) -> Result<()> {
    tokio::task::spawn_blocking(move || set_foreground_sync(hwnd))
        .await
        .context("spawn_blocking panicked")?
}

// ---------------------------------------------------------------------------
// Blocking implementations
// ---------------------------------------------------------------------------

fn enum_windows_sync() -> Result<Vec<WindowInfo>> {
    use std::sync::{Arc, Mutex};
    use windows::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
            IsWindowVisible, WNDENUMPROC,
        },
    };

    let results: Arc<Mutex<Vec<WindowInfo>>> = Arc::new(Mutex::new(Vec::new()));
    let results_clone = results.clone();

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
        let results_ptr = lparam.0 as *const Arc<Mutex<Vec<WindowInfo>>>;
        let results = unsafe { &*results_ptr };

        // Get title.
        let mut title_buf = [0u16; 512];
        let title_len =
            unsafe { GetWindowTextW(hwnd, &mut title_buf) };
        if title_len == 0 {
            return 1; // skip untitled
        }
        let title = String::from_utf16_lossy(&title_buf[..title_len as usize]);

        // Get PID.
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };

        // Get rect.
        let mut rect = windows::Win32::Foundation::RECT::default();
        let _ = unsafe { GetWindowRect(hwnd, &mut rect) };

        let visible = unsafe { IsWindowVisible(hwnd).as_bool() };

        let info = WindowInfo {
            hwnd: hwnd.0 as i64,
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
        1 // continue enumeration
    }

    let ptr = &results_clone as *const Arc<Mutex<Vec<WindowInfo>>>;
    unsafe {
        EnumWindows(
            Some(enum_proc),
            LPARAM(ptr as isize),
        ).context("EnumWindows failed")?;
    }

    let vec = Arc::try_unwrap(results)
        .unwrap_or_else(|arc| (*arc).clone().into_inner().unwrap_or_default())
        .into_inner()
        .unwrap_or_default();
    Ok(vec)
}

fn set_foreground_sync(hwnd: i64) -> Result<()> {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::SetForegroundWindow,
    };
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    unsafe {
        SetForegroundWindow(hwnd);
    }
    Ok(())
}

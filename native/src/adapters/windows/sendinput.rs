//! SendInputAdapter — Windows-specific Win32 input injection.
//!
//! Uses PostMessage(WM_KEYDOWN/WM_KEYUP) to inject key events into a specific
//! HWND without stealing focus. Useful for background game automation.
//!
//! Implements InputPort. For unfocused game injection, also exposes
//! `inject_to_hwnd()` directly.

use crate::domain::input::{InputError, Key, KeyAction, MouseEvent};
use crate::ports::InputPort;
use async_trait::async_trait;
use tracing::instrument;

pub struct SendInputAdapter;

impl SendInputAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SendInputAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InputPort for SendInputAdapter {
    #[instrument(name = "sendinput.key_event", skip(self), fields(key = %key.0, action = ?action))]
    async fn key_event(&self, key: Key, action: KeyAction) -> Result<(), InputError> {
        // Delegate to enigo for system-wide input.
        crate::adapters::enigo::EnigoInput::new()
            .key_event(key, action)
            .await
    }

    #[instrument(name = "sendinput.type_text", skip(self), fields(len = text.len()))]
    async fn type_text(&self, text: &str) -> Result<(), InputError> {
        crate::adapters::enigo::EnigoInput::new()
            .type_text(text)
            .await
    }

    #[instrument(name = "sendinput.mouse_event", skip(self))]
    async fn mouse_event(&self, event: MouseEvent) -> Result<(), InputError> {
        crate::adapters::enigo::EnigoInput::new()
            .mouse_event(event)
            .await
    }
}

/// Inject a key press directly to a game window HWND using Win32 PostMessage.
/// This bypasses focus — useful for injecting input into background game windows.
#[cfg(target_os = "windows")]
pub fn inject_to_hwnd(hwnd_value: isize, key_str: &str) -> Result<(), InputError> {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{PostMessageW, WM_KEYDOWN, WM_KEYUP},
    };

    let vk = vk_from_key_str(key_str)?;
    let hwnd = HWND(hwnd_value as *mut core::ffi::c_void);
    let scan = scan_code(vk);
    let lparam_down: isize = 1 | (scan << 16);
    let lparam_up: isize = 1 | (scan << 16) | (1 << 30) | (1 << 31);

    unsafe {
        PostMessageW(
            Some(hwnd),
            WM_KEYDOWN,
            windows::Win32::Foundation::WPARAM(vk as usize),
            windows::Win32::Foundation::LPARAM(lparam_down),
        )
        .map_err(|e| InputError::InjectionFailed(format!("PostMessageW WM_KEYDOWN: {e}")))?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        PostMessageW(
            Some(hwnd),
            WM_KEYUP,
            windows::Win32::Foundation::WPARAM(vk as usize),
            windows::Win32::Foundation::LPARAM(lparam_up),
        )
        .map_err(|e| InputError::InjectionFailed(format!("PostMessageW WM_KEYUP: {e}")))?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn vk_from_key_str(key: &str) -> Result<u32, InputError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_BACK, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F10, VK_F11, VK_F12, VK_F2,
        VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_HOME, VK_LEFT, VK_NEXT, VK_PRIOR,
        VK_RETURN, VK_RIGHT, VK_SPACE, VK_TAB, VK_UP,
    };
    let vk = match key.to_lowercase().as_str() {
        "f1" => VK_F1.0 as u32,
        "f2" => VK_F2.0 as u32,
        "f3" => VK_F3.0 as u32,
        "f4" => VK_F4.0 as u32,
        "f5" => VK_F5.0 as u32,
        "f6" => VK_F6.0 as u32,
        "f7" => VK_F7.0 as u32,
        "f8" => VK_F8.0 as u32,
        "f9" => VK_F9.0 as u32,
        "f10" => VK_F10.0 as u32,
        "f11" => VK_F11.0 as u32,
        "f12" => VK_F12.0 as u32,
        "return" | "enter" => VK_RETURN.0 as u32,
        "escape" | "esc" => VK_ESCAPE.0 as u32,
        "space" => VK_SPACE.0 as u32,
        "tab" => VK_TAB.0 as u32,
        "backspace" => VK_BACK.0 as u32,
        "delete" => VK_DELETE.0 as u32,
        "home" => VK_HOME.0 as u32,
        "end" => VK_END.0 as u32,
        "pageup" | "page_up" => VK_PRIOR.0 as u32,
        "pagedown" | "page_down" => VK_NEXT.0 as u32,
        "left" => VK_LEFT.0 as u32,
        "right" => VK_RIGHT.0 as u32,
        "up" => VK_UP.0 as u32,
        "down" => VK_DOWN.0 as u32,
        s if s.len() == 1 => s.chars().next().unwrap().to_ascii_uppercase() as u32,
        other => return Err(InputError::UnknownKey(other.to_string())),
    };
    Ok(vk)
}

#[cfg(target_os = "windows")]
fn scan_code(vk: u32) -> isize {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, MAPVK_VK_TO_VSC};
    unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC) as isize }
}

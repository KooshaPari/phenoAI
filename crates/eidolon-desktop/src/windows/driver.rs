//! Real Win32 desktop driver (`SendInput` + DXGI/GDI screenshot).
//!
//! Compiled only with `cfg(all(target_os = "windows", feature = "desktop-windows"))`.
//! wraps: windows 0.62 — https://crates.io/crates/windows

use super::capture::capture_primary_bmp;
use super::gate::require_actions_allowed;
use eidolon_core::error::PhenoError;
use eidolon_core::input::{PointerInput, TextInput};
use eidolon_core::traits::DesktopAutomator;
use eidolon_core::{AutomationEvent, Result, Viewport};
use std::mem::size_of;
use std::path::Path;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEINPUT, VIRTUAL_KEY, VK_A, VK_CONTROL, VK_DELETE, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

/// Native Windows desktop automation client (SendInput + DXGI/GDI capture).
///
/// Destructive methods require `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`.
/// Screenshots prefer DXGI Desktop Duplication, falling back to GDI BitBlt
/// (see [`super::capture`] / `EIDOLON_DESKTOP_WIN_CAPTURE`).
#[derive(Debug, Default, Clone)]
pub struct WindowsClient;

impl WindowsClient {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn screen_size() -> Result<(i32, i32)> {
        // SAFETY: GetSystemMetrics is a pure query of desktop metrics.
        let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        if width <= 0 || height <= 0 {
            return Err(PhenoError::Platform(format!(
                "GetSystemMetrics returned invalid size {width}x{height}"
            )));
        }
        Ok((width, height))
    }

    /// Map pixel coordinates to SendInput absolute units (0..=65535).
    pub(crate) fn absolute_coords(x: i32, y: i32, width: i32, height: i32) -> (i32, i32) {
        let w = (width - 1).max(1) as i64;
        let h = (height - 1).max(1) as i64;
        let ax = ((x as i64) * 65535) / w;
        let ay = ((y as i64) * 65535) / h;
        (ax.clamp(0, 65535) as i32, ay.clamp(0, 65535) as i32)
    }

    fn send_inputs(inputs: &[INPUT]) -> Result<()> {
        if inputs.is_empty() {
            return Ok(());
        }
        // SAFETY: `inputs` is a valid slice of INPUT for the duration of the call.
        let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
        if sent as usize != inputs.len() {
            return Err(PhenoError::Platform(format!(
                "SendInput injected {sent}/{} events",
                inputs.len()
            )));
        }
        Ok(())
    }

    fn mouse_input(
        dx: i32,
        dy: i32,
        flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
    ) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy,
                    mouseData: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn key_vk(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        let flags = if up {
            KEYEVENTF_KEYUP
        } else {
            Default::default()
        };
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn key_unicode(unit: u16, up: bool) -> INPUT {
        let mut flags = KEYEVENTF_UNICODE;
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: unit,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn button_flags(
        button: &Option<String>,
    ) -> (
        windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
        windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
    ) {
        match button.as_deref() {
            Some("right") => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            Some("middle") => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
            _ => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        }
    }

    fn move_absolute(&self, x: i32, y: i32) -> Result<()> {
        let (sw, sh) = Self::screen_size()?;
        let (ax, ay) = Self::absolute_coords(x, y, sw, sh);
        Self::send_inputs(&[Self::mouse_input(
            ax,
            ay,
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
        )])
    }

    fn inject_unicode_text(text: &str, delay_ms: u32) -> Result<()> {
        for ch in text.encode_utf16() {
            Self::send_inputs(&[Self::key_unicode(ch, false), Self::key_unicode(ch, true)])?;
            if delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms as u64));
            }
        }
        Ok(())
    }

    fn chord_ctrl(vk: VIRTUAL_KEY) -> Result<()> {
        Self::send_inputs(&[
            Self::key_vk(VK_CONTROL, false),
            Self::key_vk(vk, false),
            Self::key_vk(vk, true),
            Self::key_vk(VK_CONTROL, true),
        ])
    }
}

#[async_trait::async_trait]
impl DesktopAutomator for WindowsClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        let (width, height) = Self::screen_size()?;
        Ok(Viewport::new(width as u32, height as u32, 1.0))
    }

    async fn screenshot(&self, path: &str) -> Result<()> {
        require_actions_allowed("screenshot")?;
        let _backend = capture_primary_bmp(Path::new(path))?;
        Ok(())
    }

    async fn pointer(&self, event: &PointerInput) -> Result<()> {
        require_actions_allowed("pointer")?;
        let (down, up) = Self::button_flags(&event.button);

        match event.action.as_str() {
            "move" => {
                self.move_absolute(event.x, event.y)?;
            }
            "press" | "tap" => {
                self.move_absolute(event.x, event.y)?;
                Self::send_inputs(&[Self::mouse_input(0, 0, down)])?;
                if event.action == "tap" {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Self::send_inputs(&[Self::mouse_input(0, 0, up)])?;
            }
            "release" => {
                Self::send_inputs(&[Self::mouse_input(0, 0, up)])?;
            }
            "long_press" => {
                self.move_absolute(event.x, event.y)?;
                Self::send_inputs(&[Self::mouse_input(0, 0, down)])?;
                let duration = event.duration_ms.unwrap_or(500);
                std::thread::sleep(std::time::Duration::from_millis(duration as u64));
                Self::send_inputs(&[Self::mouse_input(0, 0, up)])?;
            }
            "drag" => {
                // press at current, move to target, release (macOS-parity extension)
                Self::send_inputs(&[Self::mouse_input(0, 0, down)])?;
                self.move_absolute(event.x, event.y)?;
                Self::send_inputs(&[Self::mouse_input(0, 0, up)])?;
            }
            other => {
                log::warn!("Unknown pointer action: {other}");
            }
        }

        log::debug!(
            "Pointer event executed: ({}, {}) action={}",
            event.x,
            event.y,
            event.action
        );
        Ok(())
    }

    async fn text(&self, event: &TextInput) -> Result<()> {
        require_actions_allowed("text")?;
        let delay = event.delay_ms.unwrap_or(10);

        match event.input_type.as_str() {
            "keystroke" => {
                Self::inject_unicode_text(&event.text, delay)?;
            }
            "paste" => {
                // Assumes clipboard already holds content (macOS Cmd+V parity).
                Self::chord_ctrl(VK_V)?;
            }
            "clear" => {
                Self::chord_ctrl(VK_A)?;
                Self::send_inputs(&[
                    Self::key_vk(VK_DELETE, false),
                    Self::key_vk(VK_DELETE, true),
                ])?;
            }
            other => {
                log::warn!("Unknown text input type: {other}");
            }
        }

        log::debug!(
            "Text input executed: type={} text={}",
            event.input_type,
            event.text
        );
        Ok(())
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        log::debug!("Recorded event (windows): {:?}", event);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_coords_corners() {
        let (ax, ay) = WindowsClient::absolute_coords(0, 0, 1920, 1080);
        assert_eq!((ax, ay), (0, 0));
        let (ax, ay) = WindowsClient::absolute_coords(1919, 1079, 1920, 1080);
        assert_eq!((ax, ay), (65535, 65535));
    }

    #[test]
    fn absolute_coords_midpoint() {
        let (ax, ay) = WindowsClient::absolute_coords(960, 540, 1920, 1080);
        assert!((ax - 32767).abs() < 5);
        assert!((ay - 32767).abs() < 5);
    }

    #[test]
    fn client_reports_ready() {
        let client = WindowsClient::new().expect("construct");
        use crate::windows::WindowsDesktopDriver;
        assert!(client.send_input_ready());
        assert!(client.capture_ready());
    }
}

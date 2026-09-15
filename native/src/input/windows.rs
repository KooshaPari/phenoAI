//! Windows input injection.
//!
//! System-wide input: enigo crate (sends INPUT events via SendInput).
//! Game injection: Win32 PostMessage(WM_KEYDOWN/WM_KEYUP) for background windows
//!   that don't require focus (e.g., DINOForge game automation).

use super::{ClickParams, KeyAction, KeyParams, MouseAction, MouseButton, MoveParams, ScrollParams, ScrollDirection, TypeParams};
use anyhow::{Context, Result};
use enigo::{
    Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings,
};
use tracing::debug;

// ---------------------------------------------------------------------------
// Keyboard
// ---------------------------------------------------------------------------

pub async fn key(p: KeyParams) -> Result<()> {
    let key_str = p.key.clone();
    let action = p.action.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default()).context("Failed to init Enigo")?;
        let k = parse_key(&key_str)?;
        let dir = match action {
            KeyAction::Press => Direction::Click,
            KeyAction::Down => Direction::Press,
            KeyAction::Up => Direction::Release,
        };
        debug!("input.key: {:?} {:?}", k, dir);
        enigo.key(k, dir).context("enigo.key failed")?;
        Ok(())
    })
    .await
    .context("spawn_blocking panicked")?
}

pub async fn type_text(p: TypeParams) -> Result<()> {
    let text = p.text.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default()).context("Failed to init Enigo")?;
        debug!("input.type: {} chars", text.len());
        enigo.text(&text).context("enigo.text failed")?;
        Ok(())
    })
    .await
    .context("spawn_blocking panicked")?
}

// ---------------------------------------------------------------------------
// Mouse
// ---------------------------------------------------------------------------

pub async fn click(p: ClickParams) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default()).context("Failed to init Enigo")?;
        // Move to position first.
        enigo
            .move_mouse(p.x, p.y, Coordinate::Abs)
            .context("enigo move_mouse failed")?;
        let btn = map_button(&p.button);
        let dir = match p.action {
            MouseAction::Click => Direction::Click,
            MouseAction::Down => Direction::Press,
            MouseAction::Up => Direction::Release,
        };
        debug!("input.click: ({},{}) {:?} {:?}", p.x, p.y, btn, dir);
        enigo.button(btn, dir).context("enigo.button failed")?;
        Ok(())
    })
    .await
    .context("spawn_blocking panicked")?
}

pub async fn scroll(p: ScrollParams) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default()).context("Failed to init Enigo")?;
        enigo
            .move_mouse(p.x, p.y, Coordinate::Abs)
            .context("enigo move_mouse failed")?;
        let amount = p.amount.unwrap_or(3);
        debug!("input.scroll: ({},{}) {:?} x{}", p.x, p.y, p.direction, amount);
        match p.direction {
            ScrollDirection::Up => enigo.scroll(amount, enigo::Axis::Vertical).context("scroll up failed")?,
            ScrollDirection::Down => enigo.scroll(-amount, enigo::Axis::Vertical).context("scroll down failed")?,
            ScrollDirection::Right => enigo.scroll(amount, enigo::Axis::Horizontal).context("scroll right failed")?,
            ScrollDirection::Left => enigo.scroll(-amount, enigo::Axis::Horizontal).context("scroll left failed")?,
        }
        Ok(())
    })
    .await
    .context("spawn_blocking panicked")?
}

pub async fn move_mouse(p: MoveParams) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default()).context("Failed to init Enigo")?;
        debug!("input.move: ({},{})", p.x, p.y);
        enigo
            .move_mouse(p.x, p.y, Coordinate::Abs)
            .context("enigo move_mouse failed")?;
        Ok(())
    })
    .await
    .context("spawn_blocking panicked")?
}

// ---------------------------------------------------------------------------
// Game injection: PostMessage to HWND (no focus required)
// ---------------------------------------------------------------------------

/// Inject a key press directly to a game window HWND using Win32 PostMessage.
/// This bypasses focus requirements — useful for injecting input into background
/// game windows (e.g., DINOForge driving DINO without stealing focus).
#[cfg(target_os = "windows")]
pub fn inject_to_hwnd(hwnd_value: isize, key_str: &str) -> Result<()> {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{PostMessageW, WM_KEYDOWN, WM_KEYUP},
    };

    let vk = vk_from_key_str(key_str)?;
    let hwnd = HWND(hwnd_value);
    let lparam_down: isize = 1 | (scan_code(vk) << 16);
    let lparam_up: isize = 1 | (scan_code(vk) << 16) | (1 << 30) | (1 << 31);

    unsafe {
        PostMessageW(hwnd, WM_KEYDOWN, windows::Win32::Foundation::WPARAM(vk as usize), windows::Win32::Foundation::LPARAM(lparam_down))?;
        // Small delay between down/up.
        std::thread::sleep(std::time::Duration::from_millis(50));
        PostMessageW(hwnd, WM_KEYUP, windows::Win32::Foundation::WPARAM(vk as usize), windows::Win32::Foundation::LPARAM(lparam_up))?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn vk_from_key_str(key: &str) -> Result<u32> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_F1, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_F10,
        VK_F11, VK_F12, VK_RETURN, VK_ESCAPE, VK_SPACE, VK_TAB, VK_BACK,
        VK_DELETE, VK_HOME, VK_END, VK_PRIOR, VK_NEXT,
        VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN,
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
        s if s.len() == 1 => {
            let c = s.chars().next().unwrap().to_ascii_uppercase() as u32;
            c
        }
        other => anyhow::bail!("Unknown key for PostMessage injection: {}", other),
    };
    Ok(vk)
}

#[cfg(target_os = "windows")]
fn scan_code(vk: u32) -> isize {
    use windows::Win32::UI::Input::KeyboardAndMouse::MapVirtualKeyW;
    unsafe { MapVirtualKeyW(vk, windows::Win32::UI::Input::KeyboardAndMouse::MAPVK_VK_TO_VSC) as isize }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_key(s: &str) -> Result<Key> {
    let k = match s.to_lowercase().as_str() {
        "return" | "enter" => Key::Return,
        "escape" | "esc" => Key::Escape,
        "space" => Key::Space,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "page_up" => Key::PageUp,
        "pagedown" | "page_down" => Key::PageDown,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "shift" | "lshift" => Key::Shift,
        "ctrl" | "control" | "lctrl" => Key::Control,
        "alt" | "lalt" => Key::Alt,
        "meta" | "super" | "win" => Key::Meta,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        other if other.len() == 1 => {
            Key::Unicode(other.chars().next().unwrap())
        }
        other => anyhow::bail!("Unknown key: {}", other),
    };
    Ok(k)
}

fn map_button(btn: &MouseButton) -> Button {
    match btn {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
    }
}

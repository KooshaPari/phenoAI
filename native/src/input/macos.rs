//! macOS input injection via enigo.

use super::{ClickParams, KeyAction, KeyParams, MouseAction, MouseButton, MoveParams, ScrollDirection, ScrollParams, TypeParams};
use anyhow::{Context, Result};
use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};

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
        enigo.text(&text).context("enigo.text failed")?;
        Ok(())
    })
    .await
    .context("spawn_blocking panicked")?
}

pub async fn click(p: ClickParams) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default()).context("Failed to init Enigo")?;
        enigo.move_mouse(p.x, p.y, Coordinate::Abs).context("move_mouse failed")?;
        let btn = map_button(&p.button);
        let dir = match p.action {
            MouseAction::Click => Direction::Click,
            MouseAction::Down => Direction::Press,
            MouseAction::Up => Direction::Release,
        };
        enigo.button(btn, dir).context("enigo.button failed")?;
        Ok(())
    })
    .await
    .context("spawn_blocking panicked")?
}

pub async fn scroll(p: ScrollParams) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default()).context("Failed to init Enigo")?;
        enigo.move_mouse(p.x, p.y, Coordinate::Abs).context("move_mouse failed")?;
        let amount = p.amount.unwrap_or(3);
        match p.direction {
            ScrollDirection::Up => enigo.scroll(amount, enigo::Axis::Vertical).context("scroll failed")?,
            ScrollDirection::Down => enigo.scroll(-amount, enigo::Axis::Vertical).context("scroll failed")?,
            ScrollDirection::Right => enigo.scroll(amount, enigo::Axis::Horizontal).context("scroll failed")?,
            ScrollDirection::Left => enigo.scroll(-amount, enigo::Axis::Horizontal).context("scroll failed")?,
        }
        Ok(())
    })
    .await
    .context("spawn_blocking panicked")?
}

pub async fn move_mouse(p: MoveParams) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default()).context("Failed to init Enigo")?;
        enigo.move_mouse(p.x, p.y, Coordinate::Abs).context("move_mouse failed")?;
        Ok(())
    })
    .await
    .context("spawn_blocking panicked")?
}

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
        "shift" => Key::Shift,
        "ctrl" | "control" => Key::Control,
        "alt" => Key::Alt,
        "meta" | "super" | "cmd" => Key::Meta,
        "f1" => Key::F1, "f2" => Key::F2, "f3" => Key::F3, "f4" => Key::F4,
        "f5" => Key::F5, "f6" => Key::F6, "f7" => Key::F7, "f8" => Key::F8,
        "f9" => Key::F9, "f10" => Key::F10, "f11" => Key::F11, "f12" => Key::F12,
        other if other.len() == 1 => Key::Unicode(other.chars().next().unwrap()),
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

//! EnigoInput adapter — cross-platform input injection via the enigo crate.
//! Implements InputPort for all platforms.

use crate::domain::input::{
    InputError, Key, KeyAction, MouseAction, MouseButton, MouseEvent, ScrollDirection,
};
use crate::ports::InputPort;
use async_trait::async_trait;
use enigo::{Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};
use tracing::instrument;

pub struct EnigoInput;

impl EnigoInput {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EnigoInput {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InputPort for EnigoInput {
    #[instrument(name = "enigo.key_event", skip(self), fields(key = %key.0, action = ?action))]
    async fn key_event(&self, key: Key, action: KeyAction) -> Result<(), InputError> {
        tokio::task::spawn_blocking(move || -> Result<(), InputError> {
            let mut enigo = Enigo::new(&Settings::default())
                .map_err(|e| InputError::InitFailed(e.to_string()))?;
            let k = parse_enigo_key(&key.0)?;
            let dir = map_key_direction(action);
            enigo
                .key(k, dir)
                .map_err(|e| InputError::InjectionFailed(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| InputError::InjectionFailed(format!("spawn_blocking panic: {e}")))?
    }

    #[instrument(name = "enigo.type_text", skip(self), fields(len = text.len()))]
    async fn type_text(&self, text: &str) -> Result<(), InputError> {
        let text = text.to_string();
        tokio::task::spawn_blocking(move || -> Result<(), InputError> {
            let mut enigo = Enigo::new(&Settings::default())
                .map_err(|e| InputError::InitFailed(e.to_string()))?;
            enigo
                .text(&text)
                .map_err(|e| InputError::InjectionFailed(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| InputError::InjectionFailed(format!("spawn_blocking panic: {e}")))?
    }

    #[instrument(name = "enigo.mouse_event", skip(self), fields(event = ?event))]
    async fn mouse_event(&self, event: MouseEvent) -> Result<(), InputError> {
        tokio::task::spawn_blocking(move || -> Result<(), InputError> {
            let mut enigo = Enigo::new(&Settings::default())
                .map_err(|e| InputError::InitFailed(e.to_string()))?;
            match event {
                MouseEvent::Move { x, y } => {
                    enigo
                        .move_mouse(x, y, Coordinate::Abs)
                        .map_err(|e| InputError::InjectionFailed(e.to_string()))?;
                }
                MouseEvent::Click {
                    x,
                    y,
                    button,
                    action,
                } => {
                    enigo
                        .move_mouse(x, y, Coordinate::Abs)
                        .map_err(|e| InputError::InjectionFailed(e.to_string()))?;
                    let btn = map_mouse_button(button);
                    let dir = match action {
                        MouseAction::Click => Direction::Click,
                        MouseAction::Down => Direction::Press,
                        MouseAction::Up => Direction::Release,
                    };
                    enigo
                        .button(btn, dir)
                        .map_err(|e| InputError::InjectionFailed(e.to_string()))?;
                }
                MouseEvent::Scroll {
                    x,
                    y,
                    direction,
                    amount,
                } => {
                    enigo
                        .move_mouse(x, y, Coordinate::Abs)
                        .map_err(|e| InputError::InjectionFailed(e.to_string()))?;
                    match direction {
                        ScrollDirection::Up => enigo
                            .scroll(amount, enigo::Axis::Vertical)
                            .map_err(|e| InputError::InjectionFailed(e.to_string()))?,
                        ScrollDirection::Down => enigo
                            .scroll(-amount, enigo::Axis::Vertical)
                            .map_err(|e| InputError::InjectionFailed(e.to_string()))?,
                        ScrollDirection::Right => enigo
                            .scroll(amount, enigo::Axis::Horizontal)
                            .map_err(|e| InputError::InjectionFailed(e.to_string()))?,
                        ScrollDirection::Left => enigo
                            .scroll(-amount, enigo::Axis::Horizontal)
                            .map_err(|e| InputError::InjectionFailed(e.to_string()))?,
                    }
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| InputError::InjectionFailed(format!("spawn_blocking panic: {e}")))?
    }
}

fn map_key_direction(action: KeyAction) -> Direction {
    match action {
        KeyAction::Press => Direction::Click,
        KeyAction::Down => Direction::Press,
        KeyAction::Up => Direction::Release,
    }
}

fn map_mouse_button(btn: MouseButton) -> Button {
    match btn {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
    }
}

pub(crate) fn parse_enigo_key(s: &str) -> Result<enigo::Key, InputError> {
    use enigo::Key;
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
        "meta" | "super" | "win" | "cmd" => Key::Meta,
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
        other if other.len() == 1 => Key::Unicode(other.chars().next().unwrap()),
        other => return Err(InputError::UnknownKey(other.to_string())),
    };
    Ok(k)
}

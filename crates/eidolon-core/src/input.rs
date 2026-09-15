use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PointerAction {
    Move,
    Click,
    DoubleClick,
    RightClick,
    MiddleClick,
    Scroll,
}

impl fmt::Display for PointerAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Move => write!(f, "move"),
            Self::Click => write!(f, "click"),
            Self::DoubleClick => write!(f, "double_click"),
            Self::RightClick => write!(f, "right_click"),
            Self::MiddleClick => write!(f, "middle_click"),
            Self::Scroll => write!(f, "scroll"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    Char,
    Key,
    Paste,
    IME,
}

impl fmt::Display for InputType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Char => write!(f, "char"),
            Self::Key => write!(f, "key"),
            Self::Paste => write!(f, "paste"),
            Self::IME => write!(f, "ime"),
        }
    }
}

/// Pointer (mouse/touch) input action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointerInput {
    /// X coordinate.
    pub x: i32,
    /// Y coordinate.
    pub y: i32,
    /// Button: "left", "right", "middle", or None for movement.
    pub button: Option<String>,
    /// Action type.
    pub action: PointerAction,
    /// Duration in milliseconds for long press / hold.
    pub duration_ms: Option<u32>,
}

/// Text input action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextInput {
    /// Text to input.
    pub text: String,
    /// Type of input.
    pub input_type: InputType,
    /// Delay between keystrokes (ms).
    pub delay_ms: Option<u32>,
}

impl PointerInput {
    pub fn click(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            button: Some("left".to_string()),
            action: PointerAction::Click,
            duration_ms: None,
        }
    }

    pub fn move_to(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            button: None,
            action: PointerAction::Move,
            duration_ms: None,
        }
    }
}

impl TextInput {
    pub fn keystroke(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            input_type: InputType::Key,
            delay_ms: None,
        }
    }

    pub fn paste(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            input_type: InputType::Paste,
            delay_ms: None,
        }
    }
}

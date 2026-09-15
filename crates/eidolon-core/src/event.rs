use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::input::{PointerInput, TextInput};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    KeyPress,
    KeyRelease,
    MouseMove,
    MouseClick,
    MouseScroll,
    TextInput,
    WindowFocus,
    WindowResize,
    AppSwitch,
    Custom(String),
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyPress => write!(f, "key_press"),
            Self::KeyRelease => write!(f, "key_release"),
            Self::MouseMove => write!(f, "mouse_move"),
            Self::MouseClick => write!(f, "mouse_click"),
            Self::MouseScroll => write!(f, "mouse_scroll"),
            Self::TextInput => write!(f, "text_input"),
            Self::WindowFocus => write!(f, "window_focus"),
            Self::WindowResize => write!(f, "window_resize"),
            Self::AppSwitch => write!(f, "app_switch"),
            Self::Custom(s) => write!(f, "{s}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    MacOS,
    Windows,
    Linux,
    Ios,
    Android,
    Unknown,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MacOS => write!(f, "macos"),
            Self::Windows => write!(f, "windows"),
            Self::Linux => write!(f, "linux"),
            Self::Ios => write!(f, "ios"),
            Self::Android => write!(f, "android"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Unified automation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationEvent {
    /// Event identifier.
    pub id: String,
    /// Event type.
    pub event_type: EventType,
    /// Platform.
    pub platform: Platform,
    /// Event payload.
    pub payload: EventPayload,
    /// Timestamp (Unix seconds).
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EventPayload {
    Pointer(PointerInput),
    Text(TextInput),
    Screenshot { path: String },
    Assertion { condition: String, expected: String },
    Navigate { url: String },
    Custom { data: serde_json::Value },
}

impl AutomationEvent {
    /// Create a new pointer event.
    pub fn pointer(platform: Platform, input: PointerInput) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            event_type: EventType::Custom("pointer".into()),
            platform,
            payload: EventPayload::Pointer(input),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Create a new text input event.
    pub fn text(platform: Platform, input: TextInput) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            event_type: EventType::Custom("text".into()),
            platform,
            payload: EventPayload::Text(input),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Create a screenshot event.
    pub fn screenshot(platform: Platform, path: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            event_type: EventType::Custom("screenshot".into()),
            platform,
            payload: EventPayload::Screenshot { path: path.into() },
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

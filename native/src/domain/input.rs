//! Domain types for input injection — zero external dependencies.

/// A keyboard key identifier (string-based for cross-platform portability).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key(pub String);

impl Key {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// The direction/lifecycle of a key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Full press-and-release cycle.
    Press,
    /// Key-down only.
    Down,
    /// Key-up only.
    Up,
}

/// A mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// The lifecycle of a mouse button event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    /// Full click cycle.
    Click,
    /// Button-down only.
    Down,
    /// Button-up only.
    Up,
}

/// Scroll axis and direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// A complete mouse event (move, click, or scroll).
#[derive(Debug, Clone)]
pub enum MouseEvent {
    Move {
        x: i32,
        y: i32,
    },
    Click {
        x: i32,
        y: i32,
        button: MouseButton,
        action: MouseAction,
    },
    Scroll {
        x: i32,
        y: i32,
        direction: ScrollDirection,
        amount: i32,
    },
}

/// Errors that can arise during input injection.
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("unknown key: {0}")]
    UnknownKey(String),
    #[error("injection failed: {0}")]
    InjectionFailed(String),
    #[error("device initialization failed: {0}")]
    InitFailed(String),
}

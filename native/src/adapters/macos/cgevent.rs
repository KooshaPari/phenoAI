//! CGEventAdapter — macOS input injection via enigo (CoreGraphics events).
//! Implements InputPort for macOS; delegates to EnigoInput.

use crate::adapters::enigo::EnigoInput;
use crate::domain::input::{InputError, Key, KeyAction, MouseEvent};
use crate::ports::InputPort;
use async_trait::async_trait;
use tracing::instrument;

pub struct CGEventAdapter {
    inner: EnigoInput,
}

impl CGEventAdapter {
    pub fn new() -> Self {
        Self {
            inner: EnigoInput::new(),
        }
    }
}

impl Default for CGEventAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InputPort for CGEventAdapter {
    #[instrument(name = "cgevent.key_event", skip(self))]
    async fn key_event(&self, key: Key, action: KeyAction) -> Result<(), InputError> {
        self.inner.key_event(key, action).await
    }

    #[instrument(name = "cgevent.type_text", skip(self))]
    async fn type_text(&self, text: &str) -> Result<(), InputError> {
        self.inner.type_text(text).await
    }

    #[instrument(name = "cgevent.mouse_event", skip(self))]
    async fn mouse_event(&self, event: MouseEvent) -> Result<(), InputError> {
        self.inner.mouse_event(event).await
    }
}

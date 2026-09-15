//! UinputAdapter — Linux input injection via enigo (which uses uinput/X11 internally).
//! Implements InputPort for Linux; delegates to EnigoInput.

use crate::adapters::enigo::EnigoInput;
use crate::domain::input::{InputError, Key, KeyAction, MouseEvent};
use crate::ports::InputPort;
use async_trait::async_trait;
use tracing::instrument;

pub struct UinputAdapter {
    inner: EnigoInput,
}

impl UinputAdapter {
    pub fn new() -> Self {
        Self {
            inner: EnigoInput::new(),
        }
    }
}

impl Default for UinputAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InputPort for UinputAdapter {
    #[instrument(name = "uinput.key_event", skip(self))]
    async fn key_event(&self, key: Key, action: KeyAction) -> Result<(), InputError> {
        self.inner.key_event(key, action).await
    }

    #[instrument(name = "uinput.type_text", skip(self))]
    async fn type_text(&self, text: &str) -> Result<(), InputError> {
        self.inner.type_text(text).await
    }

    #[instrument(name = "uinput.mouse_event", skip(self))]
    async fn mouse_event(&self, event: MouseEvent) -> Result<(), InputError> {
        self.inner.mouse_event(event).await
    }
}

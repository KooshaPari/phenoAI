//! Input capture port — abstract boundary for capturing input events.
//!
//! Provides an async trait [`InputCapture`] with an in-memory test adapter
//! and a wire adapter for production.

use crate::domain::input_capture::{CapturedEvent, InputCaptureError};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Input capture port — the abstract boundary for the input-capture side
/// of the PlayCua hex refactor.
#[async_trait]
pub trait InputCapture: Send + Sync {
    /// Block until the next input event is available.
    async fn next_event(&self) -> Result<CapturedEvent, InputCaptureError>;
    /// Poll up to `max_events` without blocking.
    async fn poll_events(&self, max_events: usize)
        -> Result<Vec<CapturedEvent>, InputCaptureError>;
}

/// In-memory adapter for testing — stores a queue of synthetic events that
/// tests can pre-populate and then consume.
pub struct InMemoryInputCaptureAdapter {
    events: Arc<Mutex<Vec<CapturedEvent>>>,
}

impl InMemoryInputCaptureAdapter {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Inject a synthetic event into the queue.
    pub async fn inject(&self, event: CapturedEvent) {
        self.events.lock().await.push(event);
    }

    /// Inject multiple synthetic events.
    pub async fn inject_many(&self, events: Vec<CapturedEvent>) {
        self.events.lock().await.extend(events);
    }
}

impl Default for InMemoryInputCaptureAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InputCapture for InMemoryInputCaptureAdapter {
    async fn next_event(&self) -> Result<CapturedEvent, InputCaptureError> {
        let mut events = self.events.lock().await;
        events
            .pop()
            .ok_or_else(|| InputCaptureError::CaptureFailed("no events".into()))
    }

    async fn poll_events(
        &self,
        max_events: usize,
    ) -> Result<Vec<CapturedEvent>, InputCaptureError> {
        let mut events = self.events.lock().await;
        let count = max_events.min(events.len());
        let result = events.drain(..count).rev().collect();
        Ok(result)
    }
}

/// Wire adapter for production — delegates to the real OS event capture.
///
/// Currently a stub that compiles and returns an empty result; the real
/// backend integration is a follow-up task.
pub struct WireInputCaptureAdapter;

impl WireInputCaptureAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WireInputCaptureAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InputCapture for WireInputCaptureAdapter {
    async fn next_event(&self) -> Result<CapturedEvent, InputCaptureError> {
        Err(InputCaptureError::DeviceUnavailable(
            "wire adapter not yet wired".into(),
        ))
    }

    async fn poll_events(
        &self,
        _max_events: usize,
    ) -> Result<Vec<CapturedEvent>, InputCaptureError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::input_capture::{
        CapturedKey, CapturedMouse, KeyAction, MouseAction, MouseButton,
    };

    /// The in-memory adapter must return an injected event on `next_event`.
    #[tokio::test]
    async fn in_memory_input_capture_round_trip() {
        let adapter = InMemoryInputCaptureAdapter::new();
        let event = CapturedEvent::Key(CapturedKey {
            key: "a".into(),
            action: KeyAction::Press,
        });
        adapter.inject(event.clone()).await;
        let out = adapter.next_event().await.expect("must return event");
        assert_eq!(out, event);
    }

    /// The in-memory adapter must return events in reverse injection order.
    #[tokio::test]
    async fn in_memory_input_capture_fifo_order() {
        let adapter = InMemoryInputCaptureAdapter::new();
        let e1 = CapturedEvent::Key(CapturedKey {
            key: "a".into(),
            action: KeyAction::Press,
        });
        let e2 = CapturedEvent::Key(CapturedKey {
            key: "b".into(),
            action: KeyAction::Release,
        });
        adapter.inject(e1.clone()).await;
        adapter.inject(e2.clone()).await;
        let out = adapter.next_event().await.expect("must return event");
        assert_eq!(out, e2);
    }

    /// `poll_events` must return up to `max_events` without draining the rest.
    #[tokio::test]
    async fn in_memory_input_capture_poll_events() {
        let adapter = InMemoryInputCaptureAdapter::new();
        let events = vec![
            CapturedEvent::Key(CapturedKey {
                key: "x".into(),
                action: KeyAction::Press,
            }),
            CapturedEvent::Mouse(CapturedMouse {
                x: 10,
                y: 20,
                button: Some(MouseButton::Left),
                action: Some(MouseAction::Click),
            }),
            CapturedEvent::Key(CapturedKey {
                key: "y".into(),
                action: KeyAction::Release,
            }),
        ];
        adapter.inject_many(events).await;
        let polled = adapter.poll_events(2).await.expect("poll must succeed");
        assert_eq!(polled.len(), 2);
        // Remaining event should still be accessible via next_event.
        let remaining = adapter.next_event().await.expect("must return remaining");
        assert_eq!(
            remaining,
            CapturedEvent::Key(CapturedKey {
                key: "y".into(),
                action: KeyAction::Release,
            })
        );
    }

    /// The wire adapter must compile and return an error on `next_event`.
    #[tokio::test]
    async fn wire_input_capture_compiles() {
        let adapter = WireInputCaptureAdapter::new();
        let result = adapter.next_event().await;
        assert!(result.is_err());
        let polled = adapter.poll_events(10).await.expect("poll must succeed");
        assert!(polled.is_empty());
    }

    /// `Box<dyn InputCapture>` storage must compile (object safety).
    #[tokio::test]
    async fn input_capture_is_object_safe() {
        let adapter = InMemoryInputCaptureAdapter::new();
        let event = CapturedEvent::Key(CapturedKey {
            key: "Enter".into(),
            action: KeyAction::Press,
        });
        adapter.inject(event.clone()).await;
        let s: Box<dyn InputCapture> = Box::new(adapter);
        let out = s.next_event().await.expect("boxed next_event must succeed");
        assert_eq!(out, event);
    }
}

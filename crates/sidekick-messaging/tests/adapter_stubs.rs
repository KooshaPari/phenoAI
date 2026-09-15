//! MessagingAdapter trait stubs for SIDE-FR-001 (audit remediation).
//! Traces to: docs/specs/SPEC.md#SIDE-FR-001

use sidekick_messaging::{Message, MessageProvider, MessagingAdapter, MessagingError, Result};

/// Stub adapter for contract testing — no network I/O.
struct StubAdapter {
    available: bool,
}

impl StubAdapter {
    fn available() -> Self {
        Self { available: true }
    }

    fn unavailable() -> Self {
        Self { available: false }
    }
}

impl MessagingAdapter for StubAdapter {
    fn send(&self, message: &Message) -> Result<String> {
        if !self.available {
            return Err(MessagingError::ProviderUnavailable(
                message.recipient.clone(),
            ));
        }
        Ok(format!("stub-sent:{}", message.provider))
    }

    fn is_available(&self, _recipient: &str) -> Result<bool> {
        Ok(self.available)
    }
}

#[test]
fn adapter_stub_send_succeeds_when_available() {
    let adapter = StubAdapter::available();
    let msg = Message::new("alice", "ping", MessageProvider::IMessage);
    let receipt = adapter.send(&msg).expect("stub send should succeed");
    assert!(receipt.contains("iMessage"));
}

#[test]
fn adapter_stub_send_fails_when_unavailable() {
    let adapter = StubAdapter::unavailable();
    let msg = Message::new("bob", "ping", MessageProvider::SMS);
    let err = adapter.send(&msg).unwrap_err();
    assert_eq!(err, MessagingError::ProviderUnavailable("bob".to_string()));
}

#[test]
fn adapter_stub_is_available_reflects_state() {
    let ok = StubAdapter::available();
    assert!(ok.is_available("any").unwrap());

    let no = StubAdapter::unavailable();
    assert!(!no.is_available("any").unwrap());
}

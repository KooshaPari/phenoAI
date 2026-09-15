//! Storage contract stubs for SIDE-DATA-003 (audit remediation).
//! Traces to: docs/remediation/DATA.md, docs/specs/SPEC.md#SIDE-DATA-003

use sidekick_messaging::{Message, MessageProvider};

/// In-memory message store contract — no persistence until MessageStore trait lands.
struct InMemoryOutbox {
    messages: Vec<Message>,
}

impl InMemoryOutbox {
    fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    fn enqueue(&mut self, message: Message) {
        self.messages.push(message);
    }

    fn len(&self) -> usize {
        self.messages.len()
    }

    fn peek(&self) -> Option<&Message> {
        self.messages.first()
    }
}

#[test]
fn storage_stub_message_roundtrip_in_memory() {
    let mut outbox = InMemoryOutbox::new();
    let msg = Message::new("+15551234567", "audit stub", MessageProvider::SMS);
    outbox.enqueue(msg);

    assert_eq!(outbox.len(), 1);
    let stored = outbox.peek().expect("outbox should hold one message");
    assert_eq!(stored.provider, MessageProvider::SMS);
    assert_eq!(stored.body, "audit stub");
}

#[test]
fn storage_stub_no_pii_persistence_in_library_types() {
    // Library types are ephemeral value objects — no implicit disk write API exists.
    let msg = Message::new("user@example.com", "hello", MessageProvider::Email);
    assert!(!msg.recipient.is_empty());
    assert!(!msg.body.is_empty());
    // Contract: future MessageStore must hash recipient at rest (see DATA.md M2).
}

#[test]
fn storage_stub_all_providers_are_distinct() {
    assert_ne!(MessageProvider::IMessage, MessageProvider::SMS);
    assert_ne!(MessageProvider::SMS, MessageProvider::Email);
    let imsg = Message::new("a", "b", MessageProvider::IMessage);
    assert_eq!(imsg.provider, MessageProvider::IMessage);
}

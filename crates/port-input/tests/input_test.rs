//! Integration tests for the `port-input` port trait.
//!
//! These tests exercise the trait via a third-party mock that lives in
//! this test file only — they prove the trait can be implemented by
//! any adapter without taking a dependency on the host's concrete
//! adapter types (enigo, recorded playback, WebDriver, ...).

use std::sync::Mutex;

use port_input::{InputError, InputEvent, InputSource, Key, KeyAction};

/// A mock input source with a hand-seeded event queue and a
/// configurable failure mode. Events are returned FIFO; once the
/// queue drains the next call returns either the seeded `Ok` sentinel
/// (default), `TransportClosed`, or `MalformedEvent` depending on
/// `seed_*` calls.
struct MockInputSource {
    queue: Mutex<Vec<InputEvent>>,
    fail_with: Mutex<Option<InputError>>,
    poll_count: Mutex<u32>,
}

impl MockInputSource {
    fn new() -> Self {
        Self {
            queue: Mutex::new(Vec::new()),
            fail_with: Mutex::new(None),
            poll_count: Mutex::new(0),
        }
    }

    fn push(&self, evt: InputEvent) {
        self.queue.lock().unwrap().push(evt);
    }

    fn seed_failure(&self, err: InputError) {
        *self.fail_with.lock().unwrap() = Some(err);
    }

    fn poll_count(&self) -> u32 {
        *self.poll_count.lock().unwrap()
    }
}

impl InputSource for MockInputSource {
    fn next_event(&self) -> Result<InputEvent, InputError> {
        *self.poll_count.lock().unwrap() += 1;
        if let Some(err) = self.fail_with.lock().unwrap().take() {
            return Err(err);
        }
        self.queue
            .lock()
            .unwrap()
            .pop()
            .ok_or_else(|| InputError::TransportClosed("queue empty".into()))
    }
}

/// Canonical "emits the seeded event" test from the L4 #61 spec:
/// push a known event into a mock source and verify `next_event`
/// returns it with all fields intact.
#[test]
fn input_source_emits_test_event() {
    let s = MockInputSource::new();
    s.push(InputEvent::Key {
        key: Key("Tab".into()),
        action: KeyAction::Press,
    });

    let evt = s.next_event().expect("seeded event must surface");
    assert_eq!(
        evt,
        InputEvent::Key {
            key: Key("Tab".into()),
            action: KeyAction::Press
        }
    );
    assert_eq!(s.poll_count(), 1, "next_event must be called exactly once");
}

/// `next_event` on an empty source (no events pushed) must return
/// `TransportClosed` so the application core can decide whether to
/// exit the run loop or switch to a fallback source.
#[test]
fn input_source_empty_returns_transport_closed() {
    let s = MockInputSource::new();
    let err = s
        .next_event()
        .expect_err("empty source must signal end-of-stream");
    assert_eq!(err.kind(), "transport_closed");
}

/// `next_event` must surface a seeded `MalformedEvent` error verbatim
/// so the caller can decide whether to skip-and-continue or abort.
#[test]
fn input_source_propagates_malformed_event() {
    let s = MockInputSource::new();
    s.seed_failure(InputError::MalformedEvent("bad key chord".into()));
    let err = s.next_event().expect_err("seeded failure must surface");
    assert_eq!(err.kind(), "malformed_event");
    assert!(
        err.to_string().contains("bad key chord"),
        "Display must include reason"
    );
}

/// `InputSource::next_event` must be safe to call from a worker
/// thread via `Arc<dyn InputSource>` — proves the `Send + Sync`
/// super-traits are wired correctly.
#[test]
fn input_source_trait_object_is_send_and_sync() {
    use std::sync::Arc;

    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Box<dyn InputSource>>();
    assert_send_sync::<Arc<dyn InputSource>>();

    // Seed the queue on the concrete mock BEFORE moving it into the
    // trait-object `Arc` — the trait object intentionally exposes
    // only `next_event`, so setup must happen via the concrete type.
    let mock = MockInputSource::new();
    mock.push(InputEvent::Key {
        key: Key("q".into()),
        action: KeyAction::Release,
    });
    let s: Arc<dyn InputSource> = Arc::new(mock);

    let s2 = Arc::clone(&s);
    let join = std::thread::spawn(move || s2.next_event());
    let evt = join
        .join()
        .expect("worker thread must not panic")
        .expect("event must surface");
    assert_eq!(
        evt,
        InputEvent::Key {
            key: Key("q".into()),
            action: KeyAction::Release
        }
    );
}

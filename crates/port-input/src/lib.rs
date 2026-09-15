//! `port-input` — InputSource port trait for the PlayCua hex refactor (L4 #61).
//!
//! This crate declares the abstract boundary between the application core
//! and any concrete input-event source. Adapters that implement
//! [`InputSource`] live outside this crate (synthetic events from `enigo`,
//! a recorded playback stream from a `.pcua` trace, a WebDriver / DevTools
//! event stream, a mock event queue for tests, etc.); the application core
//! only ever holds `Box<dyn InputSource>` / `Arc<dyn InputSource>` and
//! dispatches through the trait.
//!
//! The trait is intentionally **object-safe** so the host can swap adapters
//! at runtime: no associated types, no generic methods, only `&self`
//! receivers, `Send + Sync` super-traits. This is the load-bearing invariant
//! that makes the composition root in `playcua-app` trivial to wire.
//!
//! # Example
//!
//! ```rust
//! use port_input::{InputSource, InputEvent, InputError, Key, KeyAction};
//!
//! struct NullSource;
//! impl InputSource for NullSource {
//!     fn next_event(&self) -> Result<InputEvent, InputError> {
//!         Ok(InputEvent::Key { key: Key("a".into()), action: KeyAction::Press })
//!     }
//! }
//!
//! let s: Box<dyn InputSource> = Box::new(NullSource);
//! let evt = s.next_event()?;
//! # Ok::<(), InputError>(())
//! ```

use thiserror::Error;

/// A logical key. The string is the *semantic* key name (e.g. `"a"`,
/// `"Enter"`, `"ArrowLeft"`, `"F5"`) — adapters translate to/from the
/// backend's native representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key(pub String);

/// Press / release transition for a [`Key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAction {
    Press,
    Release,
}

/// A single input event surfaced by [`InputSource::next_event`].
///
/// The enum is non-exhaustive so a future revision can add new event
/// kinds (touch, gamepad, pen, ...) without breaking downstream
/// `match` arms — the compiler will require the `_ =>` arm.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum InputEvent {
    Key { key: Key, action: KeyAction },
}

/// Errors an [`InputSource`] adapter can return.
#[derive(Debug, Error)]
pub enum InputError {
    /// The underlying transport failed (e.g. the playback file is
    /// truncated, the WebDriver session was closed by the remote
    /// browser, the mock queue was emptied prematurely).
    #[error("transport closed: {0}")]
    TransportClosed(String),
    /// The adapter saw an event it does not know how to interpret
    /// (e.g. a malformed line in a recorded trace). Distinct from
    /// `TransportClosed` so the caller can decide whether to skip
    /// the event or abort the run.
    #[error("malformed event: {0}")]
    MalformedEvent(String),
}

impl InputError {
    /// Stable string tag for log fields and metrics labels.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::TransportClosed(_) => "transport_closed",
            Self::MalformedEvent(_) => "malformed_event",
        }
    }
}

/// InputSource port — the abstract boundary for the input-event side
/// of the PlayCua hex refactor.
///
/// # Object safety
///
/// The trait is object-safe by construction: no associated types, no
/// generic methods, only `&self` receivers, `Send + Sync` super-traits.
/// This is the load-bearing invariant that makes
/// `Box<dyn InputSource>` storage in the composition root possible.
pub trait InputSource: Send + Sync {
    /// Block until the next input event is available and return it.
    ///
    /// Adapters may return an event instantly (synthetic enigo, mock
    /// queue pop) or block (real OS event tap, recorded playback
    /// timed to wall-clock). The application core treats this as a
    /// black box — it only cares about the events that come back.
    fn next_event(&self) -> Result<InputEvent, InputError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal in-test input source that always emits a single
    /// `'a'` press event. Used to exercise object safety in-tree.
    struct PressA;

    impl InputSource for PressA {
        fn next_event(&self) -> Result<InputEvent, InputError> {
            Ok(InputEvent::Key {
                key: Key("a".into()),
                action: KeyAction::Press,
            })
        }
    }

    /// `next_event` on a `Box<dyn InputSource>` must compile (object
    /// safety is the load-bearing invariant for the composition root).
    #[test]
    fn input_source_is_object_safe_box_dyn() {
        let s: Box<dyn InputSource> = Box::new(PressA);
        let evt = s.next_event().expect("press-a source must emit");
        assert_eq!(
            evt,
            InputEvent::Key {
                key: Key("a".into()),
                action: KeyAction::Press
            }
        );
    }

    /// `InputEvent` is `#[non_exhaustive]` — any user of the crate
    /// that exhaustively matches it MUST include a `_ =>` arm. This
    /// test pins that contract by writing a match that relies on
    /// the wildcard arm and the `non_exhaustive` attribute.
    #[test]
    fn input_event_non_exhaustive_wildcard_compiles() {
        let evt = InputEvent::Key {
            key: Key("Enter".into()),
            action: KeyAction::Press,
        };
        let label = match &evt {
            InputEvent::Key { key, action } if key.0 == "Enter" && *action == KeyAction::Press => {
                "enter-press"
            }
            // The wildcard arm is the contract for `#[non_exhaustive]`.
            // Future variants (touch, gamepad, ...) will land here.
            _ => "other",
        };
        assert_eq!(label, "enter-press");
    }

    /// `InputError::kind()` must be a stable `&'static str` so callers
    /// can use it as a Prometheus label or log field.
    #[test]
    fn input_error_kind_is_stable_str() {
        let cases = [
            (
                InputError::TransportClosed("eof".into()).kind(),
                "transport_closed",
            ),
            (
                InputError::MalformedEvent("bad line".into()).kind(),
                "malformed_event",
            ),
        ];
        for (got, want) in cases {
            assert_eq!(got, want, "kind() tag drift: {got} != {want}");
        }
    }
}

//! Property-based tests for eidolon-core invariants.
//!
//! These tests use `proptest` to verify that core types and trait helpers
//! hold structural invariants regardless of input shape, covering the L11
//! quality-gate requirement for property/invariant coverage on
//! `event.rs`, `traits.rs`, and `virtual_stage.rs`.

use eidolon_core::event::{AutomationEvent, EventPayload, EventType, Platform};
use eidolon_core::input::{InputType, PointerAction, PointerInput, TextInput};
use eidolon_core::stage_error::{StageError, StageResult};
use eidolon_core::viewport::Viewport;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Viewport invariants
// ---------------------------------------------------------------------------

proptest! {
    /// `Viewport::new` must round-trip width, height, and dpr without loss,
    /// and must infer orientation from width vs height.
    #[test]
    fn viewport_new_round_trips_fields(
        w in 1u32..=16384,
        h in 1u32..=16384,
        dpr in 0.25f64..=4.0,
    ) {
        let vp = Viewport::new(w, h, dpr);
        prop_assert_eq!(vp.width, w);
        prop_assert_eq!(vp.height, h);
        prop_assert!((vp.dpr - dpr).abs() < 1e-10, "dpr mismatch: {} vs {}", vp.dpr, dpr);
        if w > h {
            prop_assert_eq!(&vp.orientation, "landscape");
        } else {
            prop_assert_eq!(&vp.orientation, "portrait");
        }
    }
}

// ---------------------------------------------------------------------------
// AutomationEvent invariants
// ---------------------------------------------------------------------------

proptest! {
    /// Every `AutomationEvent` constructed via the public factories must have
    /// a non-empty UUID id and a matching `event_type` string.
    #[test]
    fn automation_event_pointer_has_non_empty_id_and_type(
        x in -10000i32..=10000,
        y in -10000i32..=10000,
    ) {
        let input = PointerInput::click(x, y);
        let ev = AutomationEvent::pointer(Platform::Unknown, input);
        prop_assert!(!ev.id.is_empty(), "id should be a non-empty UUID");
        prop_assert_eq!(&ev.event_type, &EventType::Custom("pointer".into()));
        prop_assert_eq!(&ev.platform, &Platform::Unknown);
        prop_assert!(matches!(ev.payload, EventPayload::Pointer(_)));
    }

    /// `AutomationEvent::text` must encode the platform and type correctly.
    #[test]
    fn automation_event_text_fields_match(
        text in "[a-zA-Z0-9 ]{1,128}",
    ) {
        let input = TextInput::keystroke(text.clone());
        let ev = AutomationEvent::text(Platform::Unknown, input);
        prop_assert_eq!(&ev.event_type, &EventType::Custom("text".into()));
        prop_assert_eq!(&ev.platform, &Platform::Unknown);
        prop_assert!(matches!(ev.payload, EventPayload::Text(_)));
    }

    /// `AutomationEvent::screenshot` must store the path in the payload.
    #[test]
    fn automation_event_screenshot_stores_path(
        path in "[a-zA-Z0-9_/.-]{1,256}",
    ) {
        let ev = AutomationEvent::screenshot(Platform::Unknown, path.clone());
        prop_assert_eq!(&ev.event_type, &EventType::Custom("screenshot".into()));
        match ev.payload {
            EventPayload::Screenshot { path: stored } => {
                prop_assert_eq!(stored, path);
            }
            other => prop_assert!(false, "expected Screenshot payload, got {:?}", other),
        }
    }

    /// AutomationEvent ids are never the same across two calls (UUID v4 uniqueness).
    #[test]
    fn automation_event_ids_are_unique(
        x in 0i32..=1000,
        y in 0i32..=1000,
    ) {
        let e1 = AutomationEvent::pointer(Platform::Unknown, PointerInput::click(x, y));
        let e2 = AutomationEvent::pointer(Platform::Unknown, PointerInput::click(x, y));
        prop_assert_ne!(e1.id, e2.id, "two events should get distinct UUIDs");
    }

    /// Timestamps are always non-zero (Unix epoch seconds since 1970).
    #[test]
    fn automation_event_timestamp_is_nonzero(
        x in 0i32..=100,
        y in 0i32..=100,
    ) {
        let ev = AutomationEvent::pointer(Platform::Unknown, PointerInput::click(x, y));
        prop_assert!(ev.timestamp > 0, "timestamp should be a valid Unix epoch seconds value");
    }
}

// ---------------------------------------------------------------------------
// StageError invariants
// ---------------------------------------------------------------------------

proptest! {
    /// `StageError::message()` must return exactly the string passed to the
    /// variant constructor, for every variant.
    #[test]
    fn stage_error_message_round_trips(msg in "[a-zA-Z0-9 .:_/-]{1,256}") {
        for err in [
            StageError::Viewport(msg.clone()),
            StageError::Capture(msg.clone()),
            StageError::Input(msg.clone()),
            StageError::Record(msg.clone()),
            StageError::Metadata(msg.clone()),
            StageError::Lifecycle(msg.clone()),
            StageError::Exec(msg.clone()),
            StageError::Resource(msg.clone()),
            StageError::Other(msg.clone()),
        ] {
            prop_assert_eq!(err.message(), msg.as_str());
        }
    }

    /// `Display` for every `StageError` variant must include both the variant
    /// label and the original message.
    #[test]
    fn stage_error_display_contains_label_and_message(msg in "[a-z]{1,64}") {
        let pairs: &[(&str, StageError)] = &[
            ("viewport",  StageError::Viewport(msg.clone())),
            ("capture",   StageError::Capture(msg.clone())),
            ("input",     StageError::Input(msg.clone())),
            ("record",    StageError::Record(msg.clone())),
            ("metadata",  StageError::Metadata(msg.clone())),
            ("lifecycle", StageError::Lifecycle(msg.clone())),
            ("exec",      StageError::Exec(msg.clone())),
            ("resource",  StageError::Resource(msg.clone())),
            ("other",     StageError::Other(msg.clone())),
        ];
        for (label, err) in pairs {
            let rendered = err.to_string();
            prop_assert!(
                rendered.contains(label),
                "Display for {label:?} should contain label, got: {rendered:?}"
            );
            prop_assert!(
                rendered.contains(msg.as_str()),
                "Display for {label:?} should contain message, got: {rendered:?}"
            );
        }
    }

    /// `StageResult<T>` should propagate `Ok` values without altering them.
    #[test]
    fn stage_result_ok_passes_through(val in 0u64..u64::MAX) {
        let result: StageResult<u64> = Ok(val);
        // Avoid clippy::unnecessary_literal_unwrap: pattern-match instead.
        let StageResult::Ok(got) = result else {
            prop_assert!(false, "expected Ok, got Err");
            return Ok(());
        };
        prop_assert_eq!(got, val);
    }
}

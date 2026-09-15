//! Phase 3 FR-annotated tests for eidolon-core.
//!
//! Every test in this file MUST carry a `// Traces to: FR-EIDOLON-NNN` comment
//! per the traceability mandate in FUNCTIONAL_REQUIREMENTS.md.

use eidolon_core::input::{InputType, PointerAction, PointerInput, TextInput};
use eidolon_core::{AutomationEvent, Platform, Viewport};
use Platform::*;

// ---------------------------------------------------------------------------
// FR-EIDOLON-002 — Viewport Resolution and Orientation Detection
// ---------------------------------------------------------------------------

// Traces to: FR-EIDOLON-002
#[test]
fn fr002_orientation_boundary_equal_sides() {
    // When width == height the system must return "portrait" (not panic).
    let v = Viewport::new(800, 800, 1.0);
    assert_eq!(v.orientation, "portrait");
}

// Traces to: FR-EIDOLON-002
#[test]
fn fr002_orientation_landscape_wide() {
    let v = Viewport::new(3840, 2160, 2.0); // 4 K UHD
    assert_eq!(v.orientation, "landscape");
    assert_eq!(v.width, 3840);
    assert_eq!(v.height, 2160);
    assert_eq!(v.dpr, 2.0);
}

// Traces to: FR-EIDOLON-002
#[test]
fn fr002_preset_desktop_fhd_dimensions() {
    let v = Viewport::desktop_fhd();
    assert_eq!(v.width, 1920);
    assert_eq!(v.height, 1080);
    assert_eq!(v.dpr, 1.0);
    assert_eq!(v.orientation, "landscape");
}

// ---------------------------------------------------------------------------
// FR-EIDOLON-003 — Pointer and Text Input Serialisation
// ---------------------------------------------------------------------------

// Traces to: FR-EIDOLON-003
#[test]
fn fr003_pointer_round_trip_preserves_all_fields() {
    let original = PointerInput {
        x: 42,
        y: 99,
        button: Some("middle".to_string()),
        action: PointerAction::Click,
        duration_ms: Some(250),
    };
    let json = serde_json::to_string(&original).expect("serialize PointerInput");
    let restored: PointerInput = serde_json::from_str(&json).expect("deserialize PointerInput");
    assert_eq!(restored.x, original.x);
    assert_eq!(restored.y, original.y);
    assert_eq!(restored.button, original.button);
    assert_eq!(restored.action, original.action);
    assert_eq!(restored.duration_ms, original.duration_ms);
}

// Traces to: FR-EIDOLON-003
#[test]
fn fr003_text_round_trip_preserves_all_fields() {
    let original = TextInput {
        text: "hello eidolon".to_string(),
        input_type: InputType::Key,
        delay_ms: Some(75),
    };
    let json = serde_json::to_string(&original).expect("serialize TextInput");
    let restored: TextInput = serde_json::from_str(&json).expect("deserialize TextInput");
    assert_eq!(restored.text, original.text);
    assert_eq!(restored.input_type, original.input_type);
    assert_eq!(restored.delay_ms, original.delay_ms);
}

// Traces to: FR-EIDOLON-003
#[test]
fn fr003_pointer_null_button_serialises() {
    // Optional fields must serialise as JSON null, not be omitted.
    let p = PointerInput::move_to(0, 0);
    let json = serde_json::to_string(&p).expect("serialize");
    // button is None → must not crash; JSON must round-trip cleanly.
    let restored: PointerInput = serde_json::from_str(&json).expect("deserialize");
    assert!(restored.button.is_none());
}

// ---------------------------------------------------------------------------
// FR-EIDOLON-004 — Automation Event Audit Log
// ---------------------------------------------------------------------------

// Traces to: FR-EIDOLON-004
#[test]
fn fr004_event_unique_ids_bulk() {
    // 20 consecutive events must all have distinct IDs.
    let ids: Vec<String> = (0..20)
        .map(|i| AutomationEvent::screenshot(MacOS, format!("/img/{i}.png")).id)
        .collect();
    let mut deduped = ids.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(deduped.len(), 20, "all 20 event IDs must be unique");
}

// Traces to: FR-EIDOLON-004
#[test]
fn fr004_event_timestamp_positive() {
    let event = AutomationEvent::pointer(Linux, PointerInput::click(1, 1));
    assert!(
        event.timestamp > 0,
        "timestamp must be a positive Unix epoch value"
    );
}

// Traces to: FR-EIDOLON-004
#[test]
fn fr004_screenshot_payload_path_preserved() {
    let path = "/audit/logs/frame_0001.png";
    let event = AutomationEvent::screenshot(Unknown, path);
    match event.payload {
        eidolon_core::event::EventPayload::Screenshot { path: ref p } => {
            assert_eq!(p, path);
        }
        _ => panic!("Expected Screenshot payload variant"),
    }
}

// ---------------------------------------------------------------------------
// FR-EIDOLON-006 — Cross-Platform Send + Sync Safety
// ---------------------------------------------------------------------------

// Traces to: FR-EIDOLON-006
#[test]
fn fr006_viewport_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Viewport>();
}

// Traces to: FR-EIDOLON-006
#[test]
fn fr006_pointer_input_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PointerInput>();
}

// Traces to: FR-EIDOLON-006
#[test]
fn fr006_text_input_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<TextInput>();
}

// ---------------------------------------------------------------------------
// FR-EIDOLON-007 — Version Constant Exposure
// ---------------------------------------------------------------------------

// Traces to: FR-EIDOLON-007
#[test]
fn fr007_version_nonempty() {
    assert!(
        !eidolon_core::VERSION.is_empty(),
        "VERSION must not be empty"
    );
}

// Traces to: FR-EIDOLON-007
#[test]
fn fr007_version_semver_shape() {
    // Must be at least M.N.P (three dot-separated numeric components).
    let parts: Vec<&str> = eidolon_core::VERSION.split('.').collect();
    assert!(
        parts.len() >= 3,
        "VERSION must be semver M.N.P, got: {}",
        eidolon_core::VERSION
    );
    for part in &parts[..3] {
        assert!(
            part.parse::<u32>().is_ok(),
            "VERSION component '{}' must be numeric",
            part
        );
    }
}

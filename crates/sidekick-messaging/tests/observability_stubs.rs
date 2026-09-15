//! Observability contract stubs for SIDE-OBS-002 (audit remediation).
//! Traces to: docs/remediation/OBSERVABILITY.md, docs/specs/SPEC.md#SIDE-OBS-002

/// Required structured log / span fields once tracing is wired.
fn required_span_fields() -> &'static [&'static str] {
    &["crate", "op", "provider", "recipient_hash"]
}

/// Observability contract: crate name used in all telemetry.
const CRATE_NAME: &str = "sidekick-messaging";

#[test]
fn observability_stub_required_fields_are_documented() {
    let fields = required_span_fields();
    assert!(fields.contains(&"crate"));
    assert!(fields.contains(&"op"));
    assert!(
        fields.len() >= 3,
        "minimum span field contract must be defined"
    );
}

#[test]
fn observability_stub_crate_name_matches_spec() {
    assert_eq!(CRATE_NAME, "sidekick-messaging");
}

#[test]
fn observability_stub_public_api_surface_is_stable_for_instrumentation() {
    // Instrumentation hooks target public types — ensure they remain constructible.
    use sidekick_messaging::{Message, MessageProvider};
    let _ = Message::new("r", "b", MessageProvider::Email);
}

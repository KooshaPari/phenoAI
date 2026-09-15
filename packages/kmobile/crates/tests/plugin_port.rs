//! Integration tests for the `PluginPort` hexagonal port.
//!
//! `PluginPort` is the trait that drives the plugin registry
//! (load / enable / disable / unload).  These tests exercise the
//! in-memory adapter and the recording mock from a separate test
//! binary to catch trait-breaking changes.

use std::path::PathBuf;

use kmobile_core::{
    InMemoryPluginPort, KMobileError, MockPluginCall, MockPluginPort, PluginPort, PluginSource,
    PluginState,
};

/// FR-KMOBILE-PORT-PLUGIN-IT-000 — full plugin lifecycle
/// (load → enable → disable → unload) preserves the call order
/// and updates the stored state at every step.
#[tokio::test]
async fn plugin_port_in_memory_lifecycle() {
    let mut port = InMemoryPluginPort::new();

    let info = port
        .load_plugin_mut(
            "cocoapods-bridge",
            PluginSource::Registry("cocoapods".into()),
        )
        .expect("load");
    assert_eq!(info.state, PluginState::Registered);

    // Listing returns the plugin.
    let listed = port.list_plugins().await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, info.id);

    // Enable / disable round-trip.
    let enabled = port.enable_plugin_mut(&info.id).expect("enable");
    assert_eq!(enabled.state, PluginState::Enabled);

    let disabled = port.disable_plugin_mut(&info.id).expect("disable");
    assert_eq!(disabled.state, PluginState::Disabled);

    // Unload removes the plugin.
    assert!(port.unload_plugin_mut(&info.id));
    assert!(port.snapshot().is_empty());
    // Second unload is a no-op.
    assert!(!port.unload_plugin_mut(&info.id));
}

/// FR-KMOBILE-PORT-PLUGIN-IT-001 — `load_plugin_mut` validates
/// input: empty names are rejected and duplicate names are
/// rejected.
#[tokio::test]
async fn plugin_port_validates_input() {
    let mut port = InMemoryPluginPort::new();

    // Empty name → InvalidInput.
    let err = port
        .load_plugin_mut("", PluginSource::Registry("x".into()))
        .expect_err("empty name must error");
    assert!(matches!(err, KMobileError::InvalidInput(_)));

    // Whitespace-only name → InvalidInput.
    let err = port
        .load_plugin_mut("   ", PluginSource::Registry("x".into()))
        .expect_err("whitespace name must error");
    assert!(matches!(err, KMobileError::InvalidInput(_)));

    // Duplicate name → InvalidInput.
    port.load_plugin_mut("a", PluginSource::Registry("a".into()))
        .expect("first load");
    let err = port
        .load_plugin_mut("a", PluginSource::Registry("a".into()))
        .expect_err("duplicate name must error");
    assert!(matches!(err, KMobileError::InvalidInput(_)));
}

/// FR-KMOBILE-PORT-PLUGIN-IT-002 — enabling a non-existent
/// plugin through the async surface returns `InvalidInput`
/// with the offending id in the message.
#[tokio::test]
async fn plugin_port_enable_missing_plugin_errors() {
    let port = InMemoryPluginPort::new();

    let err = port.enable_plugin("nope").await.expect_err("missing");
    match err {
        KMobileError::InvalidInput(msg) => assert!(msg.contains("nope")),
        other => panic!("unexpected error: {other:?}"),
    }
}

/// FR-KMOBILE-PORT-PLUGIN-IT-003 — `MockPluginPort` records the
/// call sequence so domain tests can assert on it.
#[tokio::test]
async fn plugin_port_mock_records_call_sequence() {
    let mut mock = MockPluginPort::default();

    let info = mock.record_load("p", PluginSource::Path(PathBuf::from("/opt/p")));
    mock.record_enable(&info.id);
    mock.record_disable(&info.id);
    mock.record_unload(&info.id);

    let calls = mock.calls();
    assert_eq!(calls.len(), 4);
    assert!(matches!(
        &calls[0],
        MockPluginCall::Load { name, source: PluginSource::Path(_) } if name == "p"
    ));
    assert!(matches!(&calls[1], MockPluginCall::Enable(id) if id == &info.id));
    assert!(matches!(&calls[2], MockPluginCall::Disable(id) if id == &info.id));
    assert!(matches!(&calls[3], MockPluginCall::Unload(id) if id == &info.id));

    // The mock snapshot is empty after unload.
    assert!(mock.snapshot().is_empty());

    // reset_calls drops the log; storage stays as it was.
    mock.reset_calls();
    assert!(mock.calls().is_empty());
    assert!(mock.snapshot().is_empty());

    // A new call after reset is recorded cleanly.
    mock.record_load("q", PluginSource::Registry("q".into()));
    assert_eq!(mock.calls().len(), 1);
    assert!(matches!(
        &mock.calls()[0],
        MockPluginCall::Load { name, source: PluginSource::Registry(_) } if name == "q"
    ));
    assert_eq!(mock.snapshot().len(), 1);
}

/// FR-KMOBILE-PORT-PLUGIN-IT-004 — the trait is dyn-compatible
/// and can be shared across awaits via `Arc<dyn PluginPort>`.
#[tokio::test]
async fn plugin_port_is_dyn_compatible() {
    use std::sync::Arc;

    let port: Arc<dyn PluginPort> = Arc::new(InMemoryPluginPort::new());

    let p1 = Arc::clone(&port);
    let p2 = Arc::clone(&port);
    let h1 = tokio::spawn(async move { p1.list_plugins().await });
    let h2 = tokio::spawn(async move { p2.list_plugins().await });

    let r1 = h1.await.expect("join 1").expect("list 1");
    let r2 = h2.await.expect("join 2").expect("list 2");

    assert!(r1.is_empty());
    assert!(r2.is_empty());
}

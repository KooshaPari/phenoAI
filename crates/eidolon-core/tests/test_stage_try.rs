//! Integration tests for the `try_*` sibling methods on
//! `VirtualStage`, `MobileStage`, and `SandboxStage`.
//!
//! Each `try_*` method is a default-impl wrapper around its required
//! counterpart that converts the underlying `PhenoError` into a
//! method-specific [`StageError`] variant:
//! - `try_get_viewport` → `StageError::Viewport`
//! - `try_tap` / `try_swipe` / `try_input_text` / `try_pointer` /
//!   `try_text` → `StageError::Input`
//! - `try_exec` → `StageError::Exec`
//!
//! These tests verify the conversion contract end-to-end through a
//! mock impl that is toggled between a success path and a forced
//! `PhenoError::Timeout` (the cheapest variant — no inner data).
//!
//! See `crates/eidolon-core/src/virtual_stage.rs` for the trait
//! surface and `crates/eidolon-core/src/stage_error.rs` for the
//! `StageError` definition being asserted here.

use async_trait::async_trait;
use eidolon_core::error::PhenoError;
use eidolon_core::traits::SandboxMetadata;
use eidolon_core::{
    AutomationEvent, MobileStage, Platform, PointerInput, Result, SandboxStage, StageError,
    TextInput, Viewport, VirtualStage,
};
use Platform::*;

// -- Mock impls ---------------------------------------------------------------
//
// `AlwaysOk` returns `Ok(_)` for every required method. `AlwaysFail`
// returns `Err(PhenoError::Timeout)` for every required method. Both
// impl `MobileStage` and `SandboxStage` with the documented default
// no-op / zero-valued bodies (i.e. they do *not* override the mobile
// / sandbox methods), so the trait surface stays minimal and the
// test stays focused on the `try_*` conversion contract.

/// Mock that always succeeds — used for the success paths.
struct AlwaysOk;

/// Mock that always returns `Err(PhenoError::Timeout)` — used for the
/// error paths.
struct AlwaysFail;

#[async_trait]
impl VirtualStage for AlwaysOk {
    async fn get_viewport(&self) -> Result<Viewport> {
        Ok(Viewport::new(640, 480, 1.0))
    }
    async fn screenshot(&self, _path: &str) -> Result<()> {
        Ok(())
    }
    async fn pointer(&self, _event: &PointerInput) -> Result<()> {
        Ok(())
    }
    async fn text(&self, _event: &TextInput) -> Result<()> {
        Ok(())
    }
    async fn record_event(&self, _event: AutomationEvent) -> Result<()> {
        Ok(())
    }
}

// Mobile + sandbox sub-traits with default no-op bodies — the
// `try_*` defaults are what gets exercised by the tests below.
#[async_trait]
impl MobileStage for AlwaysOk {}
#[async_trait]
impl SandboxStage for AlwaysOk {}

#[async_trait]
impl VirtualStage for AlwaysFail {
    async fn get_viewport(&self) -> Result<Viewport> {
        Err(PhenoError::Timeout)
    }
    async fn screenshot(&self, _path: &str) -> Result<()> {
        Err(PhenoError::Timeout)
    }
    async fn pointer(&self, _event: &PointerInput) -> Result<()> {
        Err(PhenoError::Timeout)
    }
    async fn text(&self, _event: &TextInput) -> Result<()> {
        Err(PhenoError::Timeout)
    }
    async fn record_event(&self, _event: AutomationEvent) -> Result<()> {
        Err(PhenoError::Timeout)
    }
}

#[async_trait]
impl MobileStage for AlwaysFail {
    // Override the default no-op bodies so the `try_*` tests see the
    // `AlwaysFail` behaviour for mobile methods too.
    async fn tap(&self, _x: i32, _y: i32) -> Result<()> {
        Err(PhenoError::Timeout)
    }
    async fn swipe(&self, _x1: i32, _y1: i32, _x2: i32, _y2: i32) -> Result<()> {
        Err(PhenoError::Timeout)
    }
    async fn input_text(&self, _text: &str) -> Result<()> {
        Err(PhenoError::Timeout)
    }
}

#[async_trait]
impl SandboxStage for AlwaysFail {
    async fn get_metadata(&self) -> Result<SandboxMetadata> {
        Err(PhenoError::Timeout)
    }
    async fn start(&self) -> Result<()> {
        Err(PhenoError::Timeout)
    }
    async fn stop(&self) -> Result<()> {
        Err(PhenoError::Timeout)
    }
    async fn exec(&self, _cmd: &str) -> Result<String> {
        Err(PhenoError::Timeout)
    }
    async fn resource_usage(&self) -> Result<eidolon_core::traits::ResourceUsage> {
        Err(PhenoError::Timeout)
    }
}

// -- Tests --------------------------------------------------------------------

/// (1) `try_*` methods on `VirtualStage` (here `try_get_viewport` and
/// `try_screenshot`) cover both the success and error paths.
///
/// - Success: `AlwaysOk.try_get_viewport()` returns
///   `Ok(Viewport { .. })` (i.e. not an error).
/// - Error:   `AlwaysFail.try_get_viewport()` returns
///   `Err(StageError::Viewport(_))` and
///   `AlwaysFail.try_screenshot()` returns `Err(StageError::Capture(_))`.
///
/// The `try_*` defaults must convert the `PhenoError::Timeout` raised
/// by the mock into the method-specific `StageError` variant; the
/// test pins that contract.
#[tokio::test]
async fn virtual_stage_try_methods_route_errors_to_method_variant() {
    // -- Success path ---------------------------------------------------
    let ok = AlwaysOk;
    let vp = ok
        .try_get_viewport()
        .await
        .expect("AlwaysOk.try_get_viewport() should be Ok");
    assert_eq!(vp.width, 640);
    assert_eq!(vp.height, 480);

    ok.try_screenshot("/tmp/try-success.png")
        .await
        .expect("AlwaysOk.try_screenshot() should be Ok");

    ok.try_pointer(&PointerInput::click(10, 20))
        .await
        .expect("AlwaysOk.try_pointer() should be Ok");
    ok.try_text(&TextInput::keystroke("hi"))
        .await
        .expect("AlwaysOk.try_text() should be Ok");
    ok.try_record_event(AutomationEvent::screenshot(Unknown, "/x.png"))
        .await
        .expect("AlwaysOk.try_record_event() should be Ok");

    // -- Error path -----------------------------------------------------
    let fail = AlwaysFail;

    // `try_get_viewport` → `StageError::Viewport(_)`
    match fail.try_get_viewport().await {
        Err(StageError::Viewport(msg)) => {
            assert!(
                msg.contains("timeout"),
                "expected message to contain 'timeout', got {msg:?}"
            );
        }
        other => panic!("expected StageError::Viewport, got {other:?}"),
    }

    // `try_screenshot` → `StageError::Capture(_)`
    match fail.try_screenshot("/tmp/try-fail.png").await {
        Err(StageError::Capture(msg)) => {
            assert!(msg.contains("timeout"), "got {msg:?}");
        }
        other => panic!("expected StageError::Capture, got {other:?}"),
    }

    // `try_pointer` / `try_text` → `StageError::Input(_)`
    match fail.try_pointer(&PointerInput::click(0, 0)).await {
        Err(StageError::Input(msg)) => assert!(msg.contains("timeout"), "got {msg:?}"),
        other => panic!("expected StageError::Input, got {other:?}"),
    }
    match fail.try_text(&TextInput::keystroke("x")).await {
        Err(StageError::Input(msg)) => assert!(msg.contains("timeout"), "got {msg:?}"),
        other => panic!("expected StageError::Input, got {other:?}"),
    }

    // `try_record_event` → `StageError::Record(_)`
    match fail
        .try_record_event(AutomationEvent::screenshot(Unknown, "/y.png"))
        .await
    {
        Err(StageError::Record(msg)) => assert!(msg.contains("timeout"), "got {msg:?}"),
        other => panic!("expected StageError::Record, got {other:?}"),
    }
}

/// (2) `try_*` methods on `MobileStage` (here `try_tap`, `try_swipe`,
/// `try_input_text`) cover both the success and error paths.
///
/// The mobile `try_*` defaults must map any failure into
/// `StageError::Input(_)`.
#[tokio::test]
async fn mobile_stage_try_methods_route_errors_to_input_variant() {
    // -- Success path (default no-op bodies return Ok) -----------------
    let ok = AlwaysOk;
    ok.try_tap(1, 2)
        .await
        .expect("AlwaysOk.try_tap() should be Ok");
    ok.try_swipe(0, 0, 10, 20)
        .await
        .expect("AlwaysOk.try_swipe() should be Ok");
    ok.try_input_text("hello")
        .await
        .expect("AlwaysOk.try_input_text() should be Ok");

    // -- Error path ----------------------------------------------------
    let fail = AlwaysFail;

    // `try_tap` → `StageError::Input(_)`
    match fail.try_tap(100, 200).await {
        Err(StageError::Input(msg)) => assert!(msg.contains("timeout"), "got {msg:?}"),
        other => panic!("expected StageError::Input, got {other:?}"),
    }

    // `try_swipe` → `StageError::Input(_)`
    match fail.try_swipe(0, 0, 100, 200).await {
        Err(StageError::Input(msg)) => assert!(msg.contains("timeout"), "got {msg:?}"),
        other => panic!("expected StageError::Input, got {other:?}"),
    }

    // `try_input_text` → `StageError::Input(_)`
    match fail.try_input_text("hi").await {
        Err(StageError::Input(msg)) => assert!(msg.contains("timeout"), "got {msg:?}"),
        other => panic!("expected StageError::Input, got {other:?}"),
    }
}

/// (3) `try_*` methods on `SandboxStage` (here `try_get_metadata`,
/// `try_exec`, `try_resource_usage`) cover both the success and
/// error paths, and verify the method-specific `StageError` variant
/// mapping (`Metadata` for `get_metadata`, `Exec` for `exec`,
/// `Resource` for `resource_usage`).
#[tokio::test]
async fn sandbox_stage_try_methods_route_errors_to_method_variant() {
    // -- Success path ---------------------------------------------------
    let ok = AlwaysOk;

    let meta: SandboxMetadata = ok
        .try_get_metadata()
        .await
        .expect("AlwaysOk.try_get_metadata() should be Ok");
    assert_eq!(meta.id, "virtual-stage");
    assert_eq!(meta.image, "n/a");

    ok.try_start()
        .await
        .expect("AlwaysOk.try_start() should be Ok");
    ok.try_stop()
        .await
        .expect("AlwaysOk.try_stop() should be Ok");

    let out = ok
        .try_exec("ls -la")
        .await
        .expect("AlwaysOk.try_exec() should be Ok");
    assert_eq!(out, "");

    let usage = ok
        .try_resource_usage()
        .await
        .expect("AlwaysOk.try_resource_usage() should be Ok");
    assert_eq!(usage.cpu_percent, 0.0);
    assert_eq!(usage.memory_mb, 0);
    assert_eq!(usage.disk_mb, None);

    // -- Error path -----------------------------------------------------
    let fail = AlwaysFail;

    // `try_get_metadata` → `StageError::Metadata(_)`
    match fail.try_get_metadata().await {
        Err(StageError::Metadata(msg)) => assert!(msg.contains("timeout"), "got {msg:?}"),
        other => panic!("expected StageError::Metadata, got {other:?}"),
    }

    // `try_start` / `try_stop` → `StageError::Lifecycle(_)`
    match fail.try_start().await {
        Err(StageError::Lifecycle(msg)) => assert!(msg.contains("timeout"), "got {msg:?}"),
        other => panic!("expected StageError::Lifecycle, got {other:?}"),
    }
    match fail.try_stop().await {
        Err(StageError::Lifecycle(msg)) => assert!(msg.contains("timeout"), "got {msg:?}"),
        other => panic!("expected StageError::Lifecycle, got {other:?}"),
    }

    // `try_exec` → `StageError::Exec(_)`
    match fail.try_exec("ls").await {
        Err(StageError::Exec(msg)) => assert!(msg.contains("timeout"), "got {msg:?}"),
        other => panic!("expected StageError::Exec, got {other:?}"),
    }

    // `try_resource_usage` → `StageError::Resource(_)`
    match fail.try_resource_usage().await {
        Err(StageError::Resource(msg)) => assert!(msg.contains("timeout"), "got {msg:?}"),
        other => panic!("expected StageError::Resource, got {other:?}"),
    }
}

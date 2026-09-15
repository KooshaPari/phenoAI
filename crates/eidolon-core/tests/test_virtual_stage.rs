//! Integration tests for the `VirtualStage` trait surface and its
//! `MobileStage` / `SandboxStage` sub-traits.
//!
//! Exercises all three sub-traits in a single integration-test binary:
//!
//! 1. `VirtualStage` is *dispatchable* through
//!    `Vec<Box<dyn VirtualStage>>` across desktop (macOS, Windows,
//!    Linux), mobile, and sandbox impls — confirming object-safety
//!    and that the five required methods are reachable through a
//!    single trait object.
//! 2. The `MobileStage` default impls (`tap` / `swipe` / `input_text`)
//!    return `Ok(())` for any input shape, and the `SandboxStage`
//!    default impls (`get_metadata` / `start` / `stop` / `exec` /
//!    `resource_usage`) return the documented defaults.
//! 3. The trait is *implementable* for a unit struct (a `()`-shaped
//!    type) — i.e. there is no hidden `Sized` bound or lifetime leak
//!    that would block a real platform impl.
//!
//! See `crates/eidolon-core/src/virtual_stage.rs` for the trait
//! surface and the documented default-impl contracts being asserted
//! here.

use async_trait::async_trait;
use eidolon_core::traits::{ResourceUsage, SandboxMetadata};
use eidolon_core::event::Platform;
use eidolon_core::{
    AutomationEvent, MobileStage, PointerInput, Result, SandboxStage, TextInput,
    Viewport, VirtualStage,
};
use Platform::*;

// -- Mock platform clients ----------------------------------------------------
//
// Each is a zero-sized unit struct that impls `VirtualStage` (and, for
// `MobileClient` / `SandboxClient`, the corresponding sub-trait). They
// stay private to this test binary so they do not collide with the
// real client types in `eidolon-desktop` / `eidolon-mobile` /
// `eidolon-sandbox` (which are conditionally compiled and not
// reachable from `eidolon-core` tests).

/// Mock macOS desktop client.
struct MacOSClient;
/// Mock Windows desktop client.
struct WindowsClient;
/// Mock Linux desktop client.
struct LinuxClient;
/// Mock mobile client (iOS / Android).
struct MobileClient;
/// Mock sandbox client (Docker / nanoVMs / KVM).
struct SandboxClient;

#[async_trait]
impl VirtualStage for MacOSClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        Ok(Viewport::desktop_fhd())
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

#[async_trait]
impl VirtualStage for WindowsClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        Ok(Viewport::desktop_fhd())
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

#[async_trait]
impl VirtualStage for LinuxClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        Ok(Viewport::desktop_fhd())
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

#[async_trait]
impl VirtualStage for MobileClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        Ok(Viewport::mobile_fhd())
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

// `MobileStage` sub-trait — the default impls of `tap` / `swipe` /
// `input_text` (see `virtual_stage.rs:84-99`) are exercised by
// `mobile_stage_default_impls` below. No method overrides here: the
// fact that the `impl` block is empty proves a non-mobile stage can
// satisfy the trait without redefining the methods.
#[async_trait]
impl MobileStage for MobileClient {}

#[async_trait]
impl VirtualStage for SandboxClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        Ok(Viewport::desktop_fhd())
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

// `SandboxStage` sub-trait — the default impls of `get_metadata` /
// `start` / `stop` / `exec` / `resource_usage` (see
// `virtual_stage.rs:113-148`) are exercised by
// `sandbox_stage_default_impls` below. No method overrides here.
#[async_trait]
impl SandboxStage for SandboxClient {}

// -- Tests --------------------------------------------------------------------

/// (a) `VirtualStage` is dispatchable through a heterogeneous
/// `Vec<Box<dyn VirtualStage>>` of desktop (macOS, Windows, Linux),
/// mobile, and sandbox impls. Each `get_viewport` call must return
/// `Ok(_)` with a non-zero viewport — the five required methods
/// (`get_viewport`, `screenshot`, `pointer`, `text`, `record_event`)
/// are reachable through a single trait object.
#[tokio::test]
async fn dyn_virtual_stage_dispatch() {
    let stages: Vec<Box<dyn VirtualStage>> = vec![
        Box::new(MacOSClient),
        Box::new(WindowsClient),
        Box::new(LinuxClient),
        Box::new(MobileClient),
        Box::new(SandboxClient),
    ];
    assert_eq!(stages.len(), 5);

    // Required-method smoke check across the heterogeneous trait-object
    // vector — a regression in object-safety or `Send + Sync` would
    // surface here as a compile error, not a runtime failure.
    for (i, stage) in stages.iter().enumerate() {
        let viewport = stage.get_viewport().await.unwrap_or_else(|e| {
            panic!("stages[{i}].get_viewport() should be Ok, got Err({e:?})");
        });
        assert!(
            viewport.width > 0,
            "stages[{i}].viewport.width should be > 0"
        );
        assert!(
            viewport.height > 0,
            "stages[{i}].viewport.height should be > 0"
        );
        assert!(
            !viewport.orientation.is_empty(),
            "stages[{i}].viewport.orientation should be set",
        );
    }

    // Exercise the remaining four required methods through the trait
    // object — confirms dispatch reaches every required method, not
    // just `get_viewport`.
    for (i, stage) in stages.iter().enumerate() {
        assert!(
            stage.screenshot("/tmp/integration.png").await.is_ok(),
            "stages[{i}].screenshot() should be Ok",
        );
        assert!(
            stage.pointer(&PointerInput::click(10, 20)).await.is_ok(),
            "stages[{i}].pointer() should be Ok",
        );
        assert!(
            stage.text(&TextInput::keystroke("hi")).await.is_ok(),
            "stages[{i}].text() should be Ok",
        );
        assert!(
            stage
                .record_event(AutomationEvent::screenshot(Linux, "/u.png"))
                .await
                .is_ok(),
            "stages[{i}].record_event() should be Ok",
        );
    }
}

/// (b) Default impls on `MobileStage` (`tap`, `swipe`, `input_text`)
/// return `Ok(())` — the documented "empty value" contract — for any
/// input shape (boundary coordinates, empty strings, unicode / emoji).
#[tokio::test]
async fn mobile_stage_default_impls() {
    let stage = MobileClient;

    // Required-method smoke check (confirms the sub-trait wires
    // through to the required `VirtualStage` surface).
    let viewport = stage.get_viewport().await.unwrap();
    assert_eq!(viewport.width, 1080);
    assert_eq!(viewport.height, 1920);
    assert!(stage.screenshot("/tmp/mobile.png").await.is_ok());

    // `tap` default impl returns `Ok(())` for any (x, y) — interior,
    // origin, boundary, and negative coords.
    assert!(stage.tap(100, 200).await.is_ok());
    assert!(stage.tap(0, 0).await.is_ok());
    assert!(stage.tap(i32::MAX, i32::MIN).await.is_ok());
    assert!(stage.tap(-1, -1).await.is_ok());

    // `swipe` default impl returns `Ok(())` for any rectangle —
    // zero-length, negative coords, large coords.
    assert!(stage.swipe(0, 0, 100, 200).await.is_ok());
    assert!(stage.swipe(1, 2, 3, 4).await.is_ok());
    assert!(stage.swipe(-100, -200, 100, 200).await.is_ok());
    assert!(stage.swipe(0, 0, 0, 0).await.is_ok());

    // `input_text` default impl returns `Ok(())` for any text —
    // empty, ascii, unicode / emoji, and control chars.
    assert!(stage.input_text("hello").await.is_ok());
    assert!(stage.input_text("").await.is_ok());
    assert!(stage.input_text("🦀 emoji 🦀").await.is_ok());
    assert!(stage.input_text("\n\t\r").await.is_ok());
}

/// (c) Default impls on `SandboxStage` (`get_metadata`, `start`,
/// `stop`, `exec`, `resource_usage`) return `Ok(())` (or the
/// documented `SandboxMetadata` / `ResourceUsage` defaults). Asserts
/// the exact values from `virtual_stage.rs:113-148`.
#[tokio::test]
async fn sandbox_stage_default_impls() {
    let stage = SandboxClient;

    // Required-method smoke check.
    let viewport = stage.get_viewport().await.unwrap();
    assert_eq!(viewport.width, 1920);
    assert!(stage.screenshot("/tmp/sandbox.png").await.is_ok());

    // `get_metadata` returns the documented default `SandboxMetadata`
    // (id="virtual-stage", image="n/a", all limits zero / None).
    let meta: SandboxMetadata = stage.get_metadata().await.unwrap();
    assert_eq!(meta.id, "virtual-stage");
    assert_eq!(meta.image, "n/a");
    assert_eq!(meta.cpu_limit, 0);
    assert_eq!(meta.memory_limit_mb, 0);
    assert_eq!(meta.disk_limit_mb, None);

    // `start` / `stop` default impls return `Ok(())` and are
    // idempotent under repeated calls.
    assert!(stage.start().await.is_ok());
    assert!(stage.start().await.is_ok());
    assert!(stage.stop().await.is_ok());
    assert!(stage.stop().await.is_ok());

    // `exec` default impl returns `Ok(String::new())` (the "empty
    // value" contract) for any command, including the empty string.
    let out = stage.exec("ls -la").await.unwrap();
    assert_eq!(out, "");
    let out_empty = stage.exec("").await.unwrap();
    assert_eq!(out_empty, "");

    // `resource_usage` default impl returns the zero-valued
    // `ResourceUsage`.
    let usage: ResourceUsage = stage.resource_usage().await.unwrap();
    assert_eq!(usage.cpu_percent, 0.0);
    assert_eq!(usage.memory_mb, 0);
    assert_eq!(usage.disk_mb, None);
}

/// (d) Compile-time test: a unit struct impl'ing `VirtualStage` is
/// allowed by the trait surface (no hidden `Sized` bound, no lifetime
/// leaks). The struct remains zero-sized, and the five required
/// methods are callable through both concrete and trait-object
/// dispatch.
#[tokio::test]
async fn virtual_stage_compiles_for_unit_struct() {
    // The mock platform clients above are all unit structs
    // (`struct Foo;`) — the fact that this test file compiles is the
    // load-bearing assertion that `VirtualStage` is implementable
    // for a unit type. Re-confirm on the `MacOSClient` mock with
    // both concrete and trait-object dispatch.
    let stage = MacOSClient;
    assert_eq!(std::mem::size_of::<MacOSClient>(), 0);
    assert_eq!(std::mem::size_of::<MobileClient>(), 0);
    assert_eq!(std::mem::size_of::<SandboxClient>(), 0);

    // Concrete-method dispatch.
    let vp = stage.get_viewport().await.unwrap();
    assert_eq!(vp.width, 1920);
    assert_eq!(vp.height, 1080);
    assert!(stage.screenshot("/tmp/unit.png").await.is_ok());

    // Trait-object dispatch (proves object-safety).
    let dyn_ref: &dyn VirtualStage = &stage;
    let vp = dyn_ref.get_viewport().await.unwrap();
    assert_eq!(vp.width, 1920);
    assert!(dyn_ref.pointer(&PointerInput::click(10, 20)).await.is_ok());
    assert!(dyn_ref.text(&TextInput::keystroke("hi")).await.is_ok());
    assert!(dyn_ref
        .record_event(AutomationEvent::screenshot(Linux, "/u.png"))
        .await
        .is_ok());
}

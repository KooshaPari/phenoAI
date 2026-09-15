//! `VirtualStage` — unified automation surface for Eidolon.
//!
//! `VirtualStage` absorbs the three historical automator traits
//! ([`DesktopAutomator`](crate::DesktopAutomator),
//! [`MobileAutomator`](crate::MobileAutomator),
//! [`SandboxAutomator`](crate::SandboxAutomator)) behind a single async
//! surface. A consumer can hold `Arc<dyn VirtualStage>` and call the
//! five required methods (`get_viewport`, `screenshot`, `pointer`,
//! `text`, `record_event`) against any platform impl — desktop (macOS,
//! Windows, Linux), mobile (iOS, Android), or sandbox (Docker,
//! nanoVMs, KVM).
//!
//! The platform-specific extras are exposed as **sub-traits**:
//! - [`MobileStage`] — adds `tap` / `swipe` / `input_text` with default
//!   no-op impls. iOS + Android become sub-features of mobile, reachable
//!   through a single `MobileStage` that wraps the kmobile
//!   XCTest/UiAutomator adapters once those are extracted.
//! - [`SandboxStage`] — adds `get_metadata` / `start` / `stop` / `exec` /
//!   `resource_usage` with default no-op / zero impls. Docker, nanoVMs,
//!   KVM, and Firecracker become sub-features of sandbox.
//!
//! The historical [`DesktopAutomator`](crate::DesktopAutomator),
//! [`MobileAutomator`](crate::MobileAutomator), and
//! [`SandboxAutomator`](crate::SandboxAutomator) traits remain in
//! [`crate::traits`] for backward compatibility; `VirtualStage` is the
//! new unified handle consumers should hold.
//!
//! Implemented by: macOS (native), Windows (native), Linux
//! (X11/Wayland), iOS (via XCTest bridge), Android (via UiAutomator
//! bridge), Docker / nanoVMs / KVM (as `SandboxStage` sub-traits).

use std::sync::Arc;

use crate::stage_error::{StageError, StageResult};
use crate::traits::{ResourceUsage, SandboxMetadata};
use crate::{AutomationEvent, PointerInput, Result, TextInput, Viewport};

/// Unified automation surface — the single trait a consumer holds.
///
/// `VirtualStage` is a superset of the three historical automator
/// traits with the non-shared methods promoted to optional defaults on
/// the [`MobileStage`] and [`SandboxStage`] sub-traits. This lets a
/// consumer write one piece of code against `Arc<dyn VirtualStage>`
/// and apply it across desktop, mobile, and sandbox impls.
///
/// The five required methods are the load-bearing subset every impl
/// must provide:
/// - [`VirtualStage::get_viewport`] — current display dimensions.
/// - [`VirtualStage::screenshot`] — capture a frame to disk.
/// - [`VirtualStage::pointer`] — dispatch a pointer / mouse event.
/// - [`VirtualStage::text`] — dispatch a text / keystroke event.
/// - [`VirtualStage::record_event`] — record an event for audit /
///
/// Mobile- and sandbox-specific surface lives on the sub-traits
/// ([`MobileStage`], [`SandboxStage`]).
#[async_trait::async_trait]
pub trait VirtualStage: Send + Sync {
    /// Required: current viewport (resolution + DPR + orientation).
    async fn get_viewport(&self) -> Result<Viewport>;

    /// Required: capture a frame to disk (PNG on macOS/Linux; platform
    /// default elsewhere).
    async fn screenshot(&self, path: &str) -> Result<()>;

    /// Required: dispatch a pointer event (mouse for desktop, tap for
    /// mobile). Sandboxes may return `Err(Unsupported)`.
    async fn pointer(&self, event: &PointerInput) -> Result<()>;

    /// Required: dispatch a text event (keystroke / paste / IME).
    /// Sandboxes may return `Err(Unsupported)`.
    async fn text(&self, event: &TextInput) -> Result<()>;

    /// Required: record an event for audit / playback.
    async fn record_event(&self, event: AutomationEvent) -> Result<()>;

    // -- `try_*` variants -------------------------------------------------
    //
    // Each `try_*` method is the `StageError`-returning sibling of the
    // required method above. The default impl calls the required
    // method and converts the resulting `PhenoError` into the
    // method-specific [`StageError`] variant, so a caller can match on
    // the failure mode (e.g. `StageError::Viewport`) without
    // re-parsing an opaque `Display` string. Implementors do not need
    // to override these — the default is sufficient.

    /// `try_*` variant of [`VirtualStage::get_viewport`]. Returns the
    /// viewport on success, or [`StageError::Viewport`] on failure.
    async fn try_get_viewport(&self) -> StageResult<Viewport> {
        self.get_viewport()
            .await
            .map_err(|e| StageError::Viewport(e.to_string()))
    }

    /// `try_*` variant of [`VirtualStage::screenshot`]. Returns
    /// `Ok(())` on success, or [`StageError::Capture`] on failure.
    async fn try_screenshot(&self, path: &str) -> StageResult<()> {
        self.screenshot(path)
            .await
            .map_err(|e| StageError::Capture(e.to_string()))
    }

    /// `try_*` variant of [`VirtualStage::pointer`]. Returns `Ok(())`
    /// on success, or [`StageError::Input`] on failure.
    async fn try_pointer(&self, event: &PointerInput) -> StageResult<()> {
        self.pointer(event)
            .await
            .map_err(|e| StageError::Input(e.to_string()))
    }

    /// `try_*` variant of [`VirtualStage::text`]. Returns `Ok(())` on
    /// success, or [`StageError::Input`] on failure.
    async fn try_text(&self, event: &TextInput) -> StageResult<()> {
        self.text(event)
            .await
            .map_err(|e| StageError::Input(e.to_string()))
    }

    /// `try_*` variant of [`VirtualStage::record_event`]. Returns
    /// `Ok(())` on success, or [`StageError::Record`] on failure.
    async fn try_record_event(&self, event: AutomationEvent) -> StageResult<()> {
        self.record_event(event)
            .await
            .map_err(|e| StageError::Record(e.to_string()))
    }

    // -- Ergonomic construction helpers ------------------------------------
    //
    // The two helpers below — `boxed` and `Arc::from_box` — are the
    // shortest path from a concrete `T: VirtualStage` impl to a
    // uniform trait-object handle (`Box<dyn VirtualStage>` or
    // `Arc<dyn VirtualStage>`). They exist so callers do not need to
    // spell out the `Box::new(...)` / `Arc::from(...)` dance at every
    // registration or dispatch site, and so a future change to the
    // trait-object shape (e.g. adding a marker trait) only needs to be
    // made in one place.

    /// Wrap `self` in a `Box<dyn VirtualStage>`.
    ///
    /// Default-impl helper that turns a concrete `T: VirtualStage`
    /// value into a boxed trait object. Useful for callers that hold
    /// a concrete impl but need to erase the type to store it
    /// alongside other stages in a `Vec<Box<dyn VirtualStage>>` or
    /// pass it to [`StageRegistry`](crate::StageRegistry).
    ///
    /// The `where Self: Sized + 'static` bound keeps the trait
    /// object-safe: `dyn VirtualStage` does not satisfy `Sized`, so
    /// the method is unavailable on the trait object itself, only on
    /// the concrete impl. `Send + Sync` are already supertrait bounds
    /// and therefore implied — we add `'static` because the
    /// resulting `Box<dyn VirtualStage + 'static>` needs a lifetime
    /// anchor.
    fn boxed(self) -> Box<dyn VirtualStage>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

/// Extension trait that lets callers spell
/// `Arc::<dyn VirtualStage>::from_box(b)` to convert a
/// `Box<dyn VirtualStage>` into an `Arc<dyn VirtualStage>`.
///
/// Rust does not let external crates add inherent methods to
/// `std::sync::Arc`, so the helper is exposed through this trait.
/// The blanket impl is on `Arc<dyn VirtualStage>` itself — i.e. the
/// `Self` type *is* `Arc<dyn VirtualStage>` — which keeps the call
/// site short and matches the `Arc::from(_)` pattern callers
/// already use with `Box` arguments.
pub trait VirtualStageArcExt {
    /// Convert a `Box<dyn VirtualStage>` into `Arc<dyn VirtualStage>`.
    ///
    /// This is a one-line wrapper around `Arc::from(b)`; the value
    /// of having it on a trait is the uniform call site
    /// (`Arc::<dyn VirtualStage>::from_box(b)`) and a place to hang
    /// docs and a contract that does not depend on the stdlib
    /// signature staying unchanged.
    fn from_box(boxed: Box<dyn VirtualStage>) -> Arc<dyn VirtualStage> {
        Arc::from(boxed)
    }
}

impl VirtualStageArcExt for Arc<dyn VirtualStage> {}

/// Mobile sub-trait of [`VirtualStage`].
///
/// Android + iOS become sub-features of mobile, reachable through a
/// single `MobileStage` that wraps the kmobile XCTest/UiAutomator
/// adapters once those are extracted. Default impls are no-op `Ok(())`
/// so a non-mobile stage can satisfy the trait without implementing
/// the methods explicitly; mobile impls override with real behaviour.
#[async_trait::async_trait]
pub trait MobileStage: VirtualStage {
    /// Tap at screen coordinates `(x, y)`.
    async fn tap(&self, x: i32, y: i32) -> Result<()> {
        let _ = (x, y);
        Ok(())
    }

    /// Swipe from `(x1, y1)` to `(x2, y2)`.
    async fn swipe(&self, x1: i32, y1: i32, x2: i32, y2: i32) -> Result<()> {
        let _ = (x1, y1, x2, y2);
        Ok(())
    }

    /// Input text via the platform IME.
    async fn input_text(&self, text: &str) -> Result<()> {
        let _ = text;
        Ok(())
    }

    // -- `try_*` variants -------------------------------------------------
    //
    // StageError-returning siblings of the mobile-specific methods
    // above. Defaults map the underlying `PhenoError` into
    // [`StageError::Input`]; implementors do not need to override.

    /// `try_*` variant of [`MobileStage::tap`]. Returns `Ok(())` on
    /// success, or [`StageError::Input`] on failure.
    async fn try_tap(&self, x: i32, y: i32) -> StageResult<()> {
        self.tap(x, y)
            .await
            .map_err(|e| StageError::Input(e.to_string()))
    }

    /// `try_*` variant of [`MobileStage::swipe`]. Returns `Ok(())` on
    /// success, or [`StageError::Input`] on failure.
    async fn try_swipe(&self, x1: i32, y1: i32, x2: i32, y2: i32) -> StageResult<()> {
        self.swipe(x1, y1, x2, y2)
            .await
            .map_err(|e| StageError::Input(e.to_string()))
    }

    /// `try_*` variant of [`MobileStage::input_text`]. Returns
    /// `Ok(())` on success, or [`StageError::Input`] on failure.
    async fn try_input_text(&self, text: &str) -> StageResult<()> {
        self.input_text(text)
            .await
            .map_err(|e| StageError::Input(e.to_string()))
    }
}

/// Sandbox sub-trait of [`VirtualStage`].
///
/// Docker, nanoVMs, KVM, and Firecracker become sub-features of
/// sandbox, exposed through a single `SandboxStage` that the platform
/// impls (Docker container, Firecracker microVM, KVM domain, etc.)
/// implement. Default impls are no-op / zero-valued so a non-sandbox
/// stage can satisfy the trait; sandbox impls override with real
/// behaviour.
#[async_trait::async_trait]
pub trait SandboxStage: VirtualStage {
    /// Get sandbox metadata (image, resource limits).
    async fn get_metadata(&self) -> Result<SandboxMetadata> {
        Ok(SandboxMetadata {
            id: "virtual-stage".into(),
            image: "n/a".into(),
            cpu_limit: 0,
            memory_limit_mb: 0,
            disk_limit_mb: None,
        })
    }

    /// Start the sandbox.
    async fn start(&self) -> Result<()> {
        Ok(())
    }

    /// Stop the sandbox.
    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    /// Execute a command inside the sandbox; default returns an empty
    /// string.
    async fn exec(&self, cmd: &str) -> Result<String> {
        let _ = cmd;
        Ok(String::new())
    }

    /// Get current resource usage (CPU, memory, disk). Default is
    /// zeros.
    async fn resource_usage(&self) -> Result<ResourceUsage> {
        Ok(ResourceUsage {
            cpu_percent: 0.0,
            memory_mb: 0,
            disk_mb: None,
        })
    }

    // -- `try_*` variants -------------------------------------------------
    //
    // StageError-returning siblings of the sandbox-specific methods
    // above. Defaults map the underlying `PhenoError` into the
    // method-specific variant (`Metadata`, `Lifecycle`, `Exec`,
    // `Resource`); implementors do not need to override.

    /// `try_*` variant of [`SandboxStage::get_metadata`]. Returns the
    /// metadata on success, or [`StageError::Metadata`] on failure.
    async fn try_get_metadata(&self) -> StageResult<SandboxMetadata> {
        self.get_metadata()
            .await
            .map_err(|e| StageError::Metadata(e.to_string()))
    }

    /// `try_*` variant of [`SandboxStage::start`]. Returns `Ok(())` on
    /// success, or [`StageError::Lifecycle`] on failure.
    async fn try_start(&self) -> StageResult<()> {
        self.start()
            .await
            .map_err(|e| StageError::Lifecycle(e.to_string()))
    }

    /// `try_*` variant of [`SandboxStage::stop`]. Returns `Ok(())` on
    /// success, or [`StageError::Lifecycle`] on failure.
    async fn try_stop(&self) -> StageResult<()> {
        self.stop()
            .await
            .map_err(|e| StageError::Lifecycle(e.to_string()))
    }

    /// `try_*` variant of [`SandboxStage::exec`]. Returns the command
    /// output on success, or [`StageError::Exec`] on failure.
    async fn try_exec(&self, cmd: &str) -> StageResult<String> {
        self.exec(cmd)
            .await
            .map_err(|e| StageError::Exec(e.to_string()))
    }

    /// `try_*` variant of [`SandboxStage::resource_usage`]. Returns
    /// the resource usage on success, or [`StageError::Resource`] on
    /// failure.
    async fn try_resource_usage(&self) -> StageResult<ResourceUsage> {
        self.resource_usage()
            .await
            .map_err(|e| StageError::Resource(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;

    /// Zero-sized mock used to exercise the two ergonomic helpers
    /// (`VirtualStage::boxed` and `Arc::<dyn VirtualStage>::from_box`).
    /// Returns a fixed `Viewport` and no-ops every other method —
    /// the helpers only touch construction, not behaviour, so the
    /// trait body past being well-formed is irrelevant.
    struct MockStage;

    #[async_trait]
    impl VirtualStage for MockStage {
        async fn get_viewport(&self) -> Result<Viewport> {
            Ok(Viewport::new(1280, 720, 1.0))
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

    /// `VirtualStage::boxed` must turn a concrete `T: VirtualStage`
    /// into a `Box<dyn VirtualStage>` that is still dispatchable
    /// through the trait object — i.e. the helper preserves the
    /// required-method surface, it does not merely move the value
    /// behind a pointer.
    #[tokio::test]
    async fn boxed_wraps_concrete_impl_into_dyn_dispatch() {
        let stage = MockStage;

        // The helper returns the erased trait object, not the
        // concrete type — the type assertion below fails to compile
        // if a future refactor accidentally re-exposes the concrete
        // type.
        let boxed: Box<dyn VirtualStage> = stage.boxed();
        assert!(
            boxed.get_viewport().await.is_ok(),
            "boxed trait object should still be dispatchable through get_viewport"
        );

        // And it composes with the heterogeneous-collection pattern
        // the rest of the crate already relies on (see
        // `tests/test_virtual_stage.rs::dyn_virtual_stage_dispatch`).
        let stages: Vec<Box<dyn VirtualStage>> = vec![MockStage.boxed(), MockStage.boxed()];
        assert_eq!(stages.len(), 2);
        for (i, s) in stages.iter().enumerate() {
            let viewport = s
                .get_viewport()
                .await
                .unwrap_or_else(|e| panic!("stages[{i}].get_viewport() should be Ok, got {e:?}"));
            assert_eq!(viewport.width, 1280, "stages[{i}] width should round-trip");
            assert_eq!(viewport.height, 720, "stages[{i}] height should round-trip");
        }
    }

    /// `Arc::<dyn VirtualStage>::from_box` must turn a
    /// `Box<dyn VirtualStage>` into an `Arc<dyn VirtualStage>` that
    /// (a) is dispatchable, (b) shares the same allocation as the
    /// source `Box` (no copy / clone of the inner value), and (c)
    /// does not require the caller to spell out `Arc::from(_)` or
    /// `into()` at every call site.
    #[tokio::test]
    async fn arc_from_box_converts_boxed_into_shared_handle() {
        let boxed: Box<dyn VirtualStage> = MockStage.boxed();

        // Cheap sanity check before conversion: dispatch through
        // the `Box` works (otherwise the conversion test is
        // meaningless).
        assert!(boxed.get_viewport().await.is_ok());

        // Convert. The `Arc` we get back should be a fresh handle
        // onto the *same* allocation — i.e. no clone of the inner
        // `MockStage` happened during the move from `Box` to `Arc`.
        let arc: Arc<dyn VirtualStage> = Arc::<dyn VirtualStage>::from_box(boxed);
        let clone = arc.clone();
        assert!(
            Arc::ptr_eq(&arc, &clone),
            "from_box must not clone the inner value — Arc::ptr_eq should hold"
        );

        // And dispatch through the `Arc` reaches the same required
        // methods as dispatch through the source `Box` did.
        let viewport = arc
            .get_viewport()
            .await
            .expect("Arc<dyn VirtualStage> should be dispatchable through get_viewport");
        assert_eq!(viewport.width, 1280);
        assert_eq!(viewport.height, 720);
    }
}

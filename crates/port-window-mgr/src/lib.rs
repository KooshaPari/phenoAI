//! `port-window-mgr` — WindowManager port trait for the PlayCua hex
//! refactor (L4 #61).
//!
//! This crate declares the abstract boundary between the application core
//! and any concrete window-manager adapter. Adapters that implement
//! [`WindowManager`] live outside this crate (X11 EWMH, macOS NSWorkspace,
//! Windows EnumWindows + SetForegroundWindow, a headless test double, etc.);
//! the application core only ever holds `Box<dyn WindowManager>` /
//! `Arc<dyn WindowManager>` and dispatches through the trait.
//!
//! The trait is intentionally **object-safe** so the host can swap adapters
//! at runtime: no associated types, no generic methods, only `&self`
//! receivers, `Send + Sync` super-traits. This is the load-bearing invariant
//! that makes the composition root in `playcua-app` trivial to wire.
//!
//! # Example
//!
//! ```rust
//! use port_window_mgr::{WindowManager, Window, WindowId, WmError};
//!
//! struct NullWm;
//! impl WindowManager for NullWm {
//!     fn list(&self) -> Result<Vec<Window>, WmError> { Ok(Vec::new()) }
//!     fn focus(&self, _id: WindowId) -> Result<(), WmError> { Ok(()) }
//! }
//!
//! let wm: Box<dyn WindowManager> = Box::new(NullWm);
//! assert!(wm.list()?.is_empty());
//! # Ok::<(), WmError>(())
//! ```

use thiserror::Error;

/// Opaque, adapter-defined window identifier. Adapters are expected to
/// produce a value that is stable for the lifetime of the window (e.g.
/// a platform `HWND` / `WindowRef` / `xcb_window_t`). The type itself is
/// the host's responsibility — we wrap it in a `#[repr(transparent)]`
/// newtype so the trait stays object-safe and ABI-stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct WindowId(pub u64);

/// A single window returned by [`WindowManager::list`].
///
/// Fields are deliberately minimal — concrete adapters can supply richer
/// detail via additional methods (or by extending this struct in a future
/// minor version). `visible` lets the application core filter out
/// background / minimised windows without an extra round-trip to the OS.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Window {
    pub id: WindowId,
    pub title: String,
    pub pid: u32,
    pub visible: bool,
}

/// Errors a [`WindowManager`] adapter can return.
#[derive(Debug, Error)]
pub enum WmError {
    /// The adapter could not enumerate the window list (e.g. X server
    /// disconnect, accessibility permission denied, etc.).
    #[error("list failed: {0}")]
    ListFailed(String),
    /// The supplied [`WindowId`] does not correspond to a known window
    /// (it was closed between `list` and `focus`, or it never existed).
    #[error("window not found: {0:?}")]
    WindowNotFound(WindowId),
    /// The OS refused the focus request (e.g. the window is owned by
    /// another desktop session, focus-stealing prevention, etc.).
    #[error("focus denied: {0}")]
    FocusDenied(String),
}

impl WmError {
    /// Stable string tag for log fields and metrics labels.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ListFailed(_) => "list_failed",
            Self::WindowNotFound(_) => "window_not_found",
            Self::FocusDenied(_) => "focus_denied",
        }
    }
}

/// WindowManager port — the abstract boundary for the window-management
/// side of the PlayCua hex refactor.
///
/// # Object safety
///
/// The trait is object-safe by construction: no associated types, no
/// generic methods, only `&self` receivers, `Send + Sync` super-traits.
/// This is the load-bearing invariant that makes
/// `Box<dyn WindowManager>` storage in the composition root possible.
pub trait WindowManager: Send + Sync {
    /// Enumerate the current top-level windows visible to the adapter.
    /// An empty `Vec` is a valid result (headless test, no display
    /// server, fresh boot) — it MUST NOT be an error.
    fn list(&self) -> Result<Vec<Window>, WmError>;

    /// Bring the window identified by `id` to the foreground.
    /// Implementations should return `WindowNotFound` for stale ids
    /// rather than silently no-oping.
    fn focus(&self, id: WindowId) -> Result<(), WmError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal in-test window manager that always reports an empty
    /// list and always succeeds at `focus`. Used to exercise the
    /// object-safety invariant in-tree.
    struct EmptyWm;

    impl WindowManager for EmptyWm {
        fn list(&self) -> Result<Vec<Window>, WmError> {
            Ok(Vec::new())
        }
        fn focus(&self, _id: WindowId) -> Result<(), WmError> {
            Ok(())
        }
    }

    /// `list` on a fresh/empty adapter MUST return an empty `Vec`, not
    /// an error. (Adapters that wrap a headless test double, a CI
    /// sandbox, or a fresh boot all hit this path.)
    #[test]
    fn window_manager_list_empty_returns_empty_vec() {
        let wm = EmptyWm;
        let list = wm.list().expect("empty adapter must list without error");
        assert!(
            list.is_empty(),
            "expected empty list, got {} windows",
            list.len()
        );
    }

    /// `Box<dyn WindowManager>` storage must compile (object safety
    /// is the load-bearing invariant for the composition root).
    #[test]
    fn window_manager_is_object_safe_box_dyn() {
        let wm: Box<dyn WindowManager> = Box::new(EmptyWm);
        let list = wm.list().expect("boxed list must succeed");
        assert!(list.is_empty());
    }

    /// `WmError::kind()` must be a stable `&'static str` so callers
    /// can use it as a Prometheus label or log field.
    #[test]
    fn wm_error_kind_is_stable_str() {
        let id = WindowId(42);
        let cases = [
            (WmError::ListFailed("x".into()).kind(), "list_failed"),
            (WmError::WindowNotFound(id).kind(), "window_not_found"),
            (WmError::FocusDenied("nope".into()).kind(), "focus_denied"),
        ];
        for (got, want) in cases {
            assert_eq!(got, want, "kind() tag drift: {got} != {want}");
        }
    }
}

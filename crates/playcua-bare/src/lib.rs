//! playcua-bare — scaffold crate for the bare-cua core types merge (L4 #70)
//!
//! This crate is a **Phase 2 scaffolding stub**. It currently contains
//! placeholder re-exports that will be replaced by the actual bare-cua
//! domain types, port traits, and IPC wire types in Phase 3.
//!
//! ## Merge status
//!
//! - **Phase 1 (completed):** PlayCua absorbed bare-cua's core Rust crate as
//!   `playcua-native`. Binaries, Python package, and contracts were renamed.
//! - **Phase 2 (this task):** `playcua-bare` crate scaffolded as a standalone
//!   package. The crate is not yet a workspace member; it will be promoted
//!   when the real source migration happens.
//! - **Phase 3 (future):** Actual bare-cua core files will be moved into this
//!   crate, and `playcua-native` will depend on it for domain types.
//!
//! See `merge_plan.md` at the PlayCua workspace root for the full merge plan.

#![warn(missing_docs)]

/// Placeholder module for bare-cua domain types.
///
/// In Phase 3 this will contain: `Frame`, `Key`, `WindowInfo`,
/// `ProcessHandle`, `DiffResult`, etc.
pub mod domain {
    /// Placeholder struct for a captured frame.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Frame;

    /// Placeholder struct for a keyboard key.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Key;

    /// Placeholder struct for window metadata.
    #[derive(Debug, Clone, PartialEq)]
    pub struct WindowInfo;
}

/// Placeholder module for bare-cua port traits.
///
/// In Phase 3 this will contain: `CapturePort`, `InputPort`, `WindowPort`,
/// `ProcessPort`, `AnalysisPort`, etc.
pub mod ports {
    /// Placeholder trait for the capture port.
    pub trait CapturePort: Send + Sync {
        /// Placeholder method.
        fn capture(&self) -> crate::domain::Frame;
    }

    /// Placeholder trait for the input port.
    pub trait InputPort: Send + Sync {
        /// Placeholder method.
        fn send_input(&self, key: &crate::domain::Key);
    }

    /// Placeholder trait for the window port.
    pub trait WindowPort: Send + Sync {
        /// Placeholder method.
        fn list_windows(&self) -> Vec<crate::domain::WindowInfo>;
    }
}

/// Placeholder module for bare-cua IPC wire types.
///
/// In Phase 3 this will contain: `Request`, `Response`, `read_request`,
/// `write_response`, `Dispatcher`, etc.
pub mod ipc {
    /// Placeholder request type.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Request;

    /// Placeholder response type.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Response;
}

/// Placeholder module for the plugin system.
///
/// In Phase 3 this will contain: `MethodPlugin`, `PluginRegistry`, etc.
pub mod plugins {
    /// Placeholder trait for method plugins.
    pub trait MethodPlugin: Send + Sync {
        /// Placeholder method name.
        fn name(&self) -> &'static str;
    }
}

/// Re-export the placeholder domain types at crate root for convenience.
pub use domain::{Frame, Key, WindowInfo};

/// Re-export the placeholder port traits at crate root for convenience.
pub use ports::{CapturePort, InputPort, WindowPort};

/// Re-export the placeholder IPC types at crate root for convenience.
pub use ipc::{Request, Response};

/// Re-export the placeholder plugin trait at crate root for convenience.
pub use plugins::MethodPlugin;

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test — placeholder types compile and are Send + Sync.
    #[test]
    fn placeholder_types_compile_and_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Frame>();
        assert_send_sync::<Key>();
        assert_send_sync::<WindowInfo>();
        assert_send_sync::<Request>();
        assert_send_sync::<Response>();
    }

    /// Smoke test — placeholder port traits are object-safe.
    #[test]
    fn port_traits_are_object_safe() {
        fn _capture(_: Box<dyn CapturePort>) {}
        fn _input(_: Box<dyn InputPort>) {}
        fn _window(_: Box<dyn WindowPort>) {}
        // If this compiles, the traits are object-safe.
    }

    /// Smoke test — placeholder plugin trait is object-safe.
    #[test]
    fn plugin_trait_is_object_safe() {
        fn _plugin(_: Box<dyn MethodPlugin>) {}
        // If this compiles, the trait is object-safe.
    }
}

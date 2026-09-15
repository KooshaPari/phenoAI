//! Concrete wire adapter for the Renderer port.
//!
//! Re-exports the production-ready [`WireRendererAdapter`] from the port
//! module so the adapters layer is a single discovery point.

#![allow(unused_imports)]

pub use crate::ports::renderer::WireRendererAdapter;

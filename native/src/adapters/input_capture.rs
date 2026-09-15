//! Concrete wire adapter for the Input capture port.
//!
//! Re-exports the production-ready [`WireInputCaptureAdapter`] from the port
//! module so the adapters layer is a single discovery point.

#![allow(unused_imports)]

pub use crate::ports::input::WireInputCaptureAdapter;

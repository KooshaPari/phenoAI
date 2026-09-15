//! Domain layer — pure business types with zero external crate imports.
//!
//! All types here depend only on `std` and `serde` (for serialization contracts).
//! No adapter libraries (xcap, enigo, windows-rs) may be imported here.

pub mod analysis;
pub mod capture;
pub mod input;
pub mod input_capture;
pub mod process;
pub mod render;
pub mod sandbox;
pub mod window;

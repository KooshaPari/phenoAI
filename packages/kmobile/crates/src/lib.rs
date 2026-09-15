//! KMobile Core — domain types and port traits.
//!
//! This crate contains the hexagonal **ports** (trait contracts) and
//! **domain** types that all other KMobile crates depend on. No concrete
//! adapter logic lives here.

pub mod config;
pub mod error;
pub mod ports;

pub use config::Config;
pub use error::{KMobileError, Result};
pub use ports::*;

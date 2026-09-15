//! Linux AT-SPI a11y tree helpers (A+ T1).
//!
//! # Status
//!
//! - **Stub** ([`AtspiStub`]): always available; fail-loud
//!   [`codes::DESKTOP_LINUX_ATSPI_STUB`](crate::codes::DESKTOP_LINUX_ATSPI_STUB).
//! - **Real client** ([`AtspiClient`]): behind feature `desktop-linux-atspi` on
//!   `target_os = "linux"` — wraps the `atspi` crate (zbus) for bounded tree
//!   list / find-by-role / Action activate. Bus miss →
//!   [`codes::DESKTOP_LINUX_ATSPI_UNAVAILABLE`](crate::codes::DESKTOP_LINUX_ATSPI_UNAVAILABLE).
//!   Activate requires [`crate::ACTIONS_ALLOW_ENV`]`=1`.
//!
//! Additive sibling to [`super::LinuxClient`] (X11) — does **not** replace
//! [`eidolon_core::traits::DesktopAutomator`]. Not full Appium Desktop GUI or
//! W3C WebDriver wire (mobile Appium path is separate).
//!
//! // wraps: atspi 0.24 — https://crates.io/crates/atspi

mod stub;
mod types;

#[cfg(all(target_os = "linux", feature = "desktop-linux-atspi"))]
mod client;

pub use stub::AtspiStub;
pub use types::{AtspiElement, AtspiNode};

#[cfg(all(target_os = "linux", feature = "desktop-linux-atspi"))]
pub use client::AtspiClient;

use eidolon_core::Result;

/// AT-SPI accessibility-tree automator (Linux).
///
/// Read-only list/find stay ungated. [`LinuxAtspiAutomator::activate`] requires
/// `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`.
#[async_trait::async_trait]
pub trait LinuxAtspiAutomator: Send + Sync {
    /// True when connected to the AT-SPI registry / a11y bus.
    fn atspi_ready(&self) -> bool;

    /// List accessible nodes under the registry root (bounded depth/count).
    async fn list_nodes(&self, max: usize) -> Result<Vec<AtspiNode>>;

    /// Find nodes by role (case-insensitive) and optional name substring.
    async fn find_by_role(
        &self,
        role: &str,
        name: Option<&str>,
    ) -> Result<Vec<AtspiNode>>;

    /// Activate via the Action interface (click/press/activate or index 0).
    ///
    /// Requires `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`.
    async fn activate(&self, node: &AtspiNode) -> Result<()>;
}

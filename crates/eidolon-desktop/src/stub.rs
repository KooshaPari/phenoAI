//! Cross-platform fail-loud [`DesktopClient`] used when native drivers are absent.
//!
//! Compiled on non-macOS targets. Returns
//! [`PhenoError::UnsupportedPlatform`](eidolon_core::PhenoError) with documented
//! [`crate::codes`] — never silent Ok for action methods.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::traits::DesktopAutomator;
use eidolon_core::{AutomationEvent, Result, Viewport};

/// Desktop automation implementer (cross-platform **fail-loud** stub).
pub struct DesktopClient {
    platform: String,
    code: &'static str,
}

impl DesktopClient {
    /// Build a stub labeled with `platform` (e.g. `"windows"`, `"linux"`).
    ///
    /// The error code is chosen from the **compile target** (`cfg`), not the
    /// label — so tests can pass any label while assertions stay OS-accurate.
    pub fn new(platform: &str) -> Self {
        Self {
            platform: platform.to_string(),
            code: stub_code_for_target(),
        }
    }

    /// Declared target label (e.g. `"linux"`, `"windows"`). Informative only.
    pub fn platform(&self) -> &str {
        &self.platform
    }

    /// Stable [`codes`] token this stub emits on action failures.
    pub fn unsupported_code(&self) -> &'static str {
        self.code
    }

    fn unsupported(&self, method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            self.code,
            format!(
                "eidolon-desktop::DesktopClient::{method} is not implemented on \
                 this target (label={:?}; macOS has a real driver; Windows \
                 SendInput needs feature `desktop-windows` on Windows; Linux \
                 X11 needs feature `desktop-linux` on Linux — see \
                 docs/EXTRACTION_PLAN.md)",
                self.platform
            ),
        )
    }
}

fn stub_code_for_target() -> &'static str {
    if cfg!(target_os = "windows") {
        codes::DESKTOP_WIN_STUB
    } else if cfg!(target_os = "linux") {
        codes::DESKTOP_LINUX_STUB
    } else {
        codes::DESKTOP_OTHER_STUB
    }
}

#[async_trait::async_trait]
impl DesktopAutomator for DesktopClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        Err(self.unsupported("get_viewport"))
    }

    async fn screenshot(&self, _path: &str) -> Result<()> {
        Err(self.unsupported("screenshot"))
    }

    async fn pointer(&self, _event: &eidolon_core::input::PointerInput) -> Result<()> {
        Err(self.unsupported("pointer"))
    }

    async fn text(&self, _event: &eidolon_core::input::TextInput) -> Result<()> {
        Err(self.unsupported("text"))
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        // Local audit sink — no OS driver required.
        log::debug!("Recorded event (stub desktop): {:?}", event);
        Ok(())
    }
}

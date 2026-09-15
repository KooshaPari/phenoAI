//! Fail-loud Windows driver stub (feature off or non-Windows callers).

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::traits::DesktopAutomator;
use eidolon_core::{AutomationEvent, Result, Viewport};

/// Fail-loud Windows driver stub.
#[derive(Debug, Default, Clone)]
pub struct WindowsStub;

impl WindowsStub {
    pub fn new() -> Self {
        Self
    }

    fn unsupported(method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            codes::DESKTOP_WIN_STUB,
            format!(
                "WindowsDesktopDriver::{method} not implemented — enable \
                 feature `desktop-windows` (or `desktop-windows-dxgi`) on \
                 target_os=windows for SendInput + DXGI/GDI capture (see \
                 docs/EXTRACTION_PLAN.md). Stub remains for cross-compile / \
                 feature-off paths."
            ),
        )
    }
}

#[async_trait::async_trait]
impl DesktopAutomator for WindowsStub {
    async fn get_viewport(&self) -> Result<Viewport> {
        Err(Self::unsupported("get_viewport"))
    }

    async fn screenshot(&self, _path: &str) -> Result<()> {
        Err(Self::unsupported("screenshot"))
    }

    async fn pointer(&self, _event: &eidolon_core::input::PointerInput) -> Result<()> {
        Err(Self::unsupported("pointer"))
    }

    async fn text(&self, _event: &eidolon_core::input::TextInput) -> Result<()> {
        Err(Self::unsupported("text"))
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        log::debug!("Recorded event (windows stub): {:?}", event);
        Ok(())
    }
}

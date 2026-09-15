//! Fail-loud AT-SPI stub (feature off or non-Linux).

use super::{AtspiNode, LinuxAtspiAutomator};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;

/// Fail-loud AT-SPI automator stub.
///
/// Available on all platforms. Real path: feature `desktop-linux-atspi` on
/// `target_os = "linux"` → [`super::AtspiClient`].
#[derive(Debug, Default, Clone)]
pub struct AtspiStub;

impl AtspiStub {
    pub fn new() -> Self {
        Self
    }

    fn unsupported(method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            codes::DESKTOP_LINUX_ATSPI_STUB,
            format!(
                "LinuxAtspiAutomator::{method} not implemented — enable feature \
                 `desktop-linux-atspi` on target_os=linux for AT-SPI a11y \
                 list/find/activate (see docs/EXTRACTION_PLAN.md). Not full \
                 Appium Desktop / W3C WebDriver wire."
            ),
        )
    }
}

#[async_trait::async_trait]
impl LinuxAtspiAutomator for AtspiStub {
    fn atspi_ready(&self) -> bool {
        false
    }

    async fn list_nodes(&self, _max: usize) -> Result<Vec<AtspiNode>> {
        Err(Self::unsupported("list_nodes"))
    }

    async fn find_by_role(
        &self,
        _role: &str,
        _name: Option<&str>,
    ) -> Result<Vec<AtspiNode>> {
        Err(Self::unsupported("find_by_role"))
    }

    async fn activate(&self, _node: &AtspiNode) -> Result<()> {
        Err(Self::unsupported("activate"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_not_ready() {
        assert!(!AtspiStub::new().atspi_ready());
    }

    #[tokio::test]
    async fn stub_fail_loud() {
        let stub = AtspiStub::new();
        let err = stub.list_nodes(1).await.unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_LINUX_ATSPI_STUB)
        );
        let err = stub.find_by_role("button", None).await.unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_LINUX_ATSPI_STUB)
        );
        let node = AtspiNode {
            role: "push button".into(),
            name: "OK".into(),
            path: "/org/a11y/atspi/accessible/null".into(),
            bus_name: ":0.0".into(),
            description: String::new(),
            states: vec![],
        };
        let err = stub.activate(&node).await.unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_LINUX_ATSPI_STUB)
        );
    }
}

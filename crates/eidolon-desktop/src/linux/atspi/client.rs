//! Real AT-SPI client — wraps the `atspi` crate on Linux.
//!
//! // wraps: atspi 0.24 — https://crates.io/crates/atspi
//!
//! Connects to the session a11y bus / `org.a11y.atspi` registry. Fail-loud
//! with [`codes::DESKTOP_LINUX_ATSPI_UNAVAILABLE`] when the bus is missing.
//! Activate requires `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`.
//!
//! Not full Appium Desktop GUI / W3C WebDriver wire — a11y tree helpers only.

use super::{AtspiNode, LinuxAtspiAutomator};
use crate::codes;
use crate::windows::require_actions_allowed;
use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::proxy::action::ActionProxy;
use atspi::zbus;
use atspi::{AccessibilityConnection, ObjectRef};
use eidolon_core::error::PhenoError;
use eidolon_core::Result;

/// Default walk depth when listing / finding (bounded tree walk).
const DEFAULT_MAX_DEPTH: usize = 6;
/// Soft cap when callers pass `max == 0` (treat as unbounded-but-capped).
const DEFAULT_LIST_CAP: usize = 256;

/// Live AT-SPI automator (Linux + `desktop-linux-atspi`).
pub struct AtspiClient {
    conn: AccessibilityConnection,
}

impl AtspiClient {
    /// Connect to the AT-SPI registry via the session a11y bus.
    ///
    /// # Errors
    ///
    /// Returns [`codes::DESKTOP_LINUX_ATSPI_UNAVAILABLE`] when the session bus
    /// or `org.a11y.atspi` registry cannot be reached — never silently empty.
    pub async fn connect() -> Result<Self> {
        match AccessibilityConnection::new().await {
            Ok(conn) => Ok(Self { conn }),
            Err(e) => Err(unavailable(format!(
                "AT-SPI AccessibilityConnection failed: {e} — session bus or \
                 org.a11y.atspi registry unavailable (enable a11y / at-spi2-core)"
            ))),
        }
    }

    fn zbus(&self) -> &zbus::Connection {
        self.conn.connection()
    }

    async fn registry_root(&self) -> Result<AccessibleProxy<'_>> {
        AccessibleProxy::builder(self.zbus())
            .destination("org.a11y.atspi.Registry")
            .map_err(|e| platform(format!("AT-SPI registry destination: {e}")))?
            .path("/org/a11y/atspi/accessible/root")
            .map_err(|e| platform(format!("AT-SPI registry root path: {e}")))?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(|e| {
                unavailable(format!(
                    "AT-SPI registry AccessibleProxy build failed: {e}"
                ))
            })
    }

    async fn proxy_for(&self, obj: &ObjectRef) -> Result<AccessibleProxy<'_>> {
        obj.as_accessible_proxy(self.zbus())
            .await
            .map_err(|e| platform(format!("AT-SPI AccessibleProxy: {e}")))
    }

    async fn node_from_proxy(&self, proxy: &AccessibleProxy<'_>) -> Result<AtspiNode> {
        let role = match proxy.get_role().await {
            Ok(r) => r.name().to_string(),
            Err(_) => proxy
                .get_role_name()
                .await
                .unwrap_or_else(|_| "unknown".into()),
        };
        let name = proxy.name().await.unwrap_or_default();
        let description = proxy.description().await.unwrap_or_default();
        let states = match proxy.get_state().await {
            Ok(set) => set.iter().map(|s| s.to_string()).collect(),
            Err(_) => Vec::new(),
        };
        let path = proxy.inner().path().as_str().to_string();
        let bus_name = proxy.inner().destination().as_str().to_string();
        Ok(AtspiNode {
            role,
            name,
            path,
            bus_name,
            description,
            states,
        })
    }

    async fn walk(
        &self,
        max: usize,
        max_depth: usize,
        role_filter: Option<&str>,
        name_filter: Option<&str>,
    ) -> Result<Vec<AtspiNode>> {
        let cap = if max == 0 { DEFAULT_LIST_CAP } else { max };
        let root = self.registry_root().await?;
        let children = root.get_children().await.map_err(|e| {
            unavailable(format!(
                "AT-SPI registry get_children failed: {e} — a11y bus present \
                 but registry walk unavailable"
            ))
        })?;

        let mut out = Vec::new();
        let mut stack: Vec<(ObjectRef, usize)> =
            children.into_iter().map(|c| (c, 1)).collect();

        while let Some((obj, depth)) = stack.pop() {
            if out.len() >= cap {
                break;
            }
            let proxy = match self.proxy_for(&obj).await {
                Ok(p) => p,
                Err(e) => {
                    log::debug!("AT-SPI skip unreachable child: {e}");
                    continue;
                }
            };
            let node = match self.node_from_proxy(&proxy).await {
                Ok(n) => n,
                Err(e) => {
                    log::debug!("AT-SPI skip unreadable node: {e}");
                    continue;
                }
            };

            let role_ok = match role_filter {
                None => true,
                Some(want) => role_matches(&node.role, want),
            };
            let name_ok = match name_filter {
                None => true,
                Some(want) => name_matches(&node.name, want),
            };
            if role_ok && name_ok {
                out.push(node);
            }

            if depth >= max_depth || out.len() >= cap {
                continue;
            }
            if let Ok(kids) = proxy.get_children().await {
                for kid in kids.into_iter().rev() {
                    stack.push((kid, depth + 1));
                }
            }
        }

        Ok(out)
    }
}

#[async_trait::async_trait]
impl LinuxAtspiAutomator for AtspiClient {
    fn atspi_ready(&self) -> bool {
        true
    }

    async fn list_nodes(&self, max: usize) -> Result<Vec<AtspiNode>> {
        self.walk(max, DEFAULT_MAX_DEPTH, None, None).await
    }

    async fn find_by_role(
        &self,
        role: &str,
        name: Option<&str>,
    ) -> Result<Vec<AtspiNode>> {
        self.walk(DEFAULT_LIST_CAP, DEFAULT_MAX_DEPTH, Some(role), name)
            .await
    }

    async fn activate(&self, node: &AtspiNode) -> Result<()> {
        require_actions_allowed("atspi_activate")?;

        let proxy = ActionProxy::builder(self.zbus())
            .destination(node.bus_name.as_str())
            .map_err(|e| platform(format!("AT-SPI Action destination: {e}")))?
            .path(node.path.as_str())
            .map_err(|e| platform(format!("AT-SPI Action path: {e}")))?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .map_err(|e| {
                platform(format!(
                    "AT-SPI ActionProxy build failed for {} {}: {e}",
                    node.bus_name, node.path
                ))
            })?;

        // Prefer a named click/press/activate; else first (default) action.
        let index = pick_action_index(&proxy).await?;
        let ok = proxy.do_action(index).await.map_err(|e| {
            platform(format!(
                "AT-SPI do_action({index}) failed for {} {}: {e}",
                node.bus_name, node.path
            ))
        })?;
        if !ok {
            return Err(platform(format!(
                "AT-SPI do_action({index}) returned false for {} {}",
                node.bus_name, node.path
            )));
        }
        Ok(())
    }
}

async fn pick_action_index(proxy: &ActionProxy<'_>) -> Result<i32> {
    let n = proxy.nactions().await.unwrap_or(0);
    if n <= 0 {
        return Err(platform(
            "AT-SPI Action interface reports nactions=0 — cannot activate",
        ));
    }
    for i in 0..n {
        if let Ok(name) = proxy.get_name(i).await {
            let lower = name.to_ascii_lowercase();
            if lower == "click" || lower == "press" || lower == "activate" {
                return Ok(i);
            }
        }
    }
    Ok(0)
}

fn role_matches(actual: &str, want: &str) -> bool {
    let a = actual.to_ascii_lowercase();
    let w = want.to_ascii_lowercase();
    a == w || a.replace(' ', "") == w.replace(' ', "") || a.contains(&w)
}

fn name_matches(actual: &str, want: &str) -> bool {
    actual.to_ascii_lowercase().contains(&want.to_ascii_lowercase())
}

fn unavailable(message: impl Into<String>) -> PhenoError {
    PhenoError::unsupported_platform(codes::DESKTOP_LINUX_ATSPI_UNAVAILABLE, message)
}

fn platform(message: impl Into<String>) -> PhenoError {
    PhenoError::Platform(message.into())
}

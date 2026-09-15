//! Public AT-SPI node types (platform-agnostic).

/// One node from the AT-SPI accessibility tree.
///
/// `path` is the D-Bus object path; `bus_name` is the destination unique name
/// (or well-known name) needed to re-bind proxies for activate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtspiNode {
    /// AT-SPI role name (English, e.g. `"push button"`).
    pub role: String,
    /// Accessible name property (may be empty).
    pub name: String,
    /// D-Bus object path (e.g. `/org/a11y/atspi/accessible/…`).
    pub path: String,
    /// D-Bus destination (application unique name or registry).
    pub bus_name: String,
    /// Longer description property (may be empty).
    pub description: String,
    /// State names currently set on the node.
    pub states: Vec<String>,
}

/// Alias kept for callers that prefer "element" wording.
pub type AtspiElement = AtspiNode;

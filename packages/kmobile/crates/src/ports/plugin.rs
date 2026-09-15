//! Plugin registry port.
//!
//! Hexagonal port for KMobile's **plugin registry**. Adapters
//! (file-system scanner, network-federated registry, in-process
//! loader, …) live in the consuming crates and implement the
//! [`PluginPort`] trait. The domain depends only on the trait and
//! the DTOs declared here.
//!
//! A plugin is anything that extends KMobile at runtime: a custom
//! device driver, a CI hook, a new MCP tool set, a third-party
//! testing framework integration. The port covers the full
//! lifecycle — discover / load / enable / disable / unload — and
//! advertises a stable set of [`PluginCapability`] values that
//! adapters can pattern-match on.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::KMobileError;

// ────────────────────────────────────────────────────────────────────────────
// DTOs
// ────────────────────────────────────────────────────────────────────────────

/// Lifecycle state of a plugin as observed by the port.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginState {
    /// Plugin source has been registered but not yet loaded into
    /// the runtime.
    Registered,
    /// Plugin source has been loaded into the runtime but is
    /// inactive (won't be invoked).
    Loaded,
    /// Plugin is loaded and active — its hooks / tools fire.
    Enabled,
    /// Plugin is loaded but inactive.
    Disabled,
    /// Plugin has been removed from the runtime.
    Unloaded,
    /// Plugin failed to load or failed at runtime.
    Failed,
}

/// Coarse capability a plugin advertises. The list is intentionally
/// small and stable; new kinds go in [`PluginCapability::Custom`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    /// Plugin extends device control.
    DeviceControl,
    /// Plugin extends simulator control.
    SimulatorControl,
    /// Plugin extends test automation.
    TestAutomation,
    /// Plugin contributes new MCP tools.
    McpTool,
    /// Escape hatch — anything not covered above.
    Custom(String),
}

/// Where a plugin was loaded from. Used by adapters to re-resolve
/// the plugin on restart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginSource {
    /// Plugin was loaded from a local file or directory.
    Path(PathBuf),
    /// Plugin was loaded by a well-known name from a federated
    /// registry. The path / URL is resolved by the adapter.
    Registry(String),
}

/// Description of a single plugin registered with the port.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginInfo {
    /// Stable plugin id (e.g. `"kmobile-cocoapods-bridge"`).
    pub id: String,
    /// Human-readable plugin name.
    pub name: String,
    /// Semantic-version string advertised by the plugin.
    pub version: String,
    /// Current lifecycle state.
    pub state: PluginState,
    /// Capabilities the plugin declares.
    pub capabilities: Vec<PluginCapability>,
    /// Where the plugin was loaded from.
    pub source: PluginSource,
    /// Optional human-readable error message — populated when
    /// `state == PluginState::Failed`.
    pub error: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// Port
// ────────────────────────────────────────────────────────────────────────────

/// Hexagonal port: discover, load, enable, disable, and unload
/// KMobile plugins. The domain layer never depends on a specific
/// plugin mechanism (wasm, dynamic library, RPC) — that decision
/// belongs to the adapter.
#[async_trait]
pub trait PluginPort: Send + Sync {
    /// List every plugin the port currently knows about, in any
    /// state.
    async fn list_plugins(&self) -> Result<Vec<PluginInfo>, KMobileError>;

    /// Look up a plugin by id.
    async fn get_plugin(&self, id: &str) -> Result<Option<PluginInfo>, KMobileError>;

    /// Register a plugin from `source`. The adapter assigns the
    /// canonical id and returns the stored [`PluginInfo`]. The
    /// plugin is in [`PluginState::Registered`] after this call —
    /// call [`PluginPort::enable_plugin`] to make it active.
    async fn load_plugin(
        &self,
        name: &str,
        source: PluginSource,
    ) -> Result<PluginInfo, KMobileError>;

    /// Move a plugin from [`PluginState::Registered`] /
    /// [`PluginState::Disabled`] to [`PluginState::Enabled`].
    async fn enable_plugin(&self, id: &str) -> Result<PluginInfo, KMobileError>;

    /// Move a plugin from [`PluginState::Enabled`] to
    /// [`PluginState::Disabled`].
    async fn disable_plugin(&self, id: &str) -> Result<PluginInfo, KMobileError>;

    /// Remove a plugin from the runtime entirely. Returns `true`
    /// if the plugin was present.
    async fn unload_plugin(&self, id: &str) -> Result<bool, KMobileError>;
}

// ────────────────────────────────────────────────────────────────────────────
// Adapter: InMemoryPluginPort
// ────────────────────────────────────────────────────────────────────────────

/// Default in-memory adapter. Used in tests, CI, and as the
/// canonical null-adapter when no plugin mechanism is wired in.
#[derive(Debug, Default, Clone)]
pub struct InMemoryPluginPort {
    plugins: Vec<PluginInfo>,
}

impl InMemoryPluginPort {
    /// Create an empty in-memory plugin port.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the current plugin list.
    pub fn snapshot(&self) -> Vec<PluginInfo> {
        self.plugins.clone()
    }
}

#[async_trait]
impl PluginPort for InMemoryPluginPort {
    async fn list_plugins(&self) -> Result<Vec<PluginInfo>, KMobileError> {
        Ok(self.plugins.clone())
    }

    async fn get_plugin(&self, id: &str) -> Result<Option<PluginInfo>, KMobileError> {
        Ok(self.plugins.iter().find(|p| p.id == id).cloned())
    }

    async fn load_plugin(
        &self,
        name: &str,
        source: PluginSource,
    ) -> Result<PluginInfo, KMobileError> {
        if name.trim().is_empty() {
            return Err(KMobileError::InvalidInput(
                "plugin name must not be empty".into(),
            ));
        }
        // The async surface takes &self, so we cannot persist
        // here. Production adapters hold interior mutability; tests
        // use `load_plugin_mut`.
        Ok(PluginInfo {
            id: format!("plugin-{}", self.plugins.len() + 1),
            name: name.into(),
            version: "0.0.0".into(),
            state: PluginState::Registered,
            capabilities: Vec::new(),
            source,
            error: None,
        })
    }

    async fn enable_plugin(&self, id: &str) -> Result<PluginInfo, KMobileError> {
        match self.plugins.iter().find(|p| p.id == id) {
            Some(plugin) => Ok(PluginInfo {
                state: PluginState::Enabled,
                ..plugin.clone()
            }),
            None => Err(KMobileError::InvalidInput(format!(
                "plugin not found: {id}"
            ))),
        }
    }

    async fn disable_plugin(&self, id: &str) -> Result<PluginInfo, KMobileError> {
        match self.plugins.iter().find(|p| p.id == id) {
            Some(plugin) => Ok(PluginInfo {
                state: PluginState::Disabled,
                ..plugin.clone()
            }),
            None => Err(KMobileError::InvalidInput(format!(
                "plugin not found: {id}"
            ))),
        }
    }

    async fn unload_plugin(&self, _id: &str) -> Result<bool, KMobileError> {
        // &self forbids mutation; production adapters persist.
        Ok(false)
    }
}

impl InMemoryPluginPort {
    /// Synchronous load used by tests holding `&mut self`.
    pub fn load_plugin_mut(
        &mut self,
        name: &str,
        source: PluginSource,
    ) -> Result<PluginInfo, KMobileError> {
        if name.trim().is_empty() {
            return Err(KMobileError::InvalidInput(
                "plugin name must not be empty".into(),
            ));
        }
        if self.plugins.iter().any(|p| p.name == name) {
            return Err(KMobileError::InvalidInput(format!(
                "plugin name already registered: {name}"
            )));
        }
        let info = PluginInfo {
            id: format!("plugin-{}", self.plugins.len() + 1),
            name: name.into(),
            version: "0.0.0".into(),
            state: PluginState::Registered,
            capabilities: Vec::new(),
            source,
            error: None,
        };
        self.plugins.push(info.clone());
        Ok(info)
    }

    /// Synchronous enable used by tests.
    pub fn enable_plugin_mut(&mut self, id: &str) -> Result<PluginInfo, KMobileError> {
        let plugin = self
            .plugins
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| KMobileError::InvalidInput(format!("plugin not found: {id}")))?;
        plugin.state = PluginState::Enabled;
        Ok(plugin.clone())
    }

    /// Synchronous disable used by tests.
    pub fn disable_plugin_mut(&mut self, id: &str) -> Result<PluginInfo, KMobileError> {
        let plugin = self
            .plugins
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| KMobileError::InvalidInput(format!("plugin not found: {id}")))?;
        plugin.state = PluginState::Disabled;
        Ok(plugin.clone())
    }

    /// Synchronous unload used by tests.
    pub fn unload_plugin_mut(&mut self, id: &str) -> bool {
        if let Some(pos) = self.plugins.iter().position(|p| p.id == id) {
            self.plugins.remove(pos);
            true
        } else {
            false
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Test mock
// ────────────────────────────────────────────────────────────────────────────

/// Recording mock that tracks every call into the port. Domain
/// tests use this when they need to assert "the domain loaded
/// `cocoapods`, then enabled it, then unloaded it".
#[derive(Debug, Default, Clone)]
pub struct MockPluginPort {
    plugins: Vec<PluginInfo>,
    calls: Vec<MockPluginCall>,
}

/// One recorded call into [`MockPluginPort`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockPluginCall {
    /// `load_plugin` was invoked with the given name and source.
    Load {
        /// The plugin name that was passed to load_plugin.
        name: String,
        /// The plugin source that was passed to load_plugin.
        source: PluginSource,
    },
    /// `enable_plugin` was invoked with the given plugin id.
    Enable(String),
    /// `disable_plugin` was invoked with the given plugin id.
    Disable(String),
    /// `unload_plugin` was invoked with the given plugin id.
    Unload(String),
}

impl MockPluginPort {
    /// Borrow the recorded call list.
    pub fn calls(&self) -> &[MockPluginCall] {
        &self.calls
    }

    /// Reset the call log (keeps the plugin list intact).
    pub fn reset_calls(&mut self) {
        self.calls.clear();
    }

    /// Snapshot the current plugin list.
    pub fn snapshot(&self) -> Vec<PluginInfo> {
        self.plugins.clone()
    }
}

#[async_trait]
impl PluginPort for MockPluginPort {
    async fn list_plugins(&self) -> Result<Vec<PluginInfo>, KMobileError> {
        Ok(self.plugins.clone())
    }

    async fn get_plugin(&self, id: &str) -> Result<Option<PluginInfo>, KMobileError> {
        Ok(self.plugins.iter().find(|p| p.id == id).cloned())
    }

    async fn load_plugin(
        &self,
        name: &str,
        _source: PluginSource,
    ) -> Result<PluginInfo, KMobileError> {
        // &self forbids mutation; tests use `record_load`.
        Ok(PluginInfo {
            id: format!("mock-{}", name),
            name: name.into(),
            version: "0.0.0".into(),
            state: PluginState::Registered,
            capabilities: Vec::new(),
            source: PluginSource::Registry(name.into()),
            error: None,
        })
    }

    async fn enable_plugin(&self, _id: &str) -> Result<PluginInfo, KMobileError> {
        // &self forbids mutation; tests use `record_enable`.
        Err(KMobileError::Unknown(
            "MockPluginPort: use record_enable from &mut self".into(),
        ))
    }

    async fn disable_plugin(&self, _id: &str) -> Result<PluginInfo, KMobileError> {
        Err(KMobileError::Unknown(
            "MockPluginPort: use record_disable from &mut self".into(),
        ))
    }

    async fn unload_plugin(&self, _id: &str) -> Result<bool, KMobileError> {
        // &self forbids mutation; tests use `record_unload`.
        Ok(true)
    }
}

impl MockPluginPort {
    /// Record a load call (used by tests holding `&mut self`).
    pub fn record_load(&mut self, name: &str, source: PluginSource) -> PluginInfo {
        self.calls.push(MockPluginCall::Load {
            name: name.into(),
            source: source.clone(),
        });
        let info = PluginInfo {
            id: format!("mock-{}", name),
            name: name.into(),
            version: "0.0.0".into(),
            state: PluginState::Registered,
            capabilities: Vec::new(),
            source,
            error: None,
        };
        self.plugins.push(info.clone());
        info
    }

    /// Record an enable call (used by tests holding `&mut self`).
    pub fn record_enable(&mut self, id: &str) -> Option<PluginInfo> {
        self.calls.push(MockPluginCall::Enable(id.into()));
        let plugin = self.plugins.iter_mut().find(|p| p.id == id)?;
        plugin.state = PluginState::Enabled;
        Some(plugin.clone())
    }

    /// Record a disable call (used by tests holding `&mut self`).
    pub fn record_disable(&mut self, id: &str) -> Option<PluginInfo> {
        self.calls.push(MockPluginCall::Disable(id.into()));
        let plugin = self.plugins.iter_mut().find(|p| p.id == id)?;
        plugin.state = PluginState::Disabled;
        Some(plugin.clone())
    }

    /// Record an unload call (used by tests holding `&mut self`).
    pub fn record_unload(&mut self, id: &str) -> bool {
        self.calls.push(MockPluginCall::Unload(id.into()));
        if let Some(pos) = self.plugins.iter().position(|p| p.id == id) {
            self.plugins.remove(pos);
            true
        } else {
            false
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Unit tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// FR-KMOBILE-PORT-PLUGIN-000 — load_plugin stores the plugin
    /// in `Registered` state and list returns it.
    #[tokio::test]
    async fn in_memory_load_and_list() {
        let mut port = InMemoryPluginPort::new();
        let info = port
            .load_plugin_mut(
                "cocoapods-bridge",
                PluginSource::Registry("cocoapods-bridge".into()),
            )
            .expect("load");
        assert_eq!(info.state, PluginState::Registered);
        assert_eq!(info.name, "cocoapods-bridge");

        let listed = port.list_plugins().await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, info.id);
    }

    /// FR-KMOBILE-PORT-PLUGIN-001 — empty plugin name is rejected
    /// with `InvalidInput`.
    #[tokio::test]
    async fn empty_name_rejected() {
        let mut port = InMemoryPluginPort::new();
        let err = port
            .load_plugin_mut("", PluginSource::Registry("x".into()))
            .unwrap_err();
        assert!(matches!(err, KMobileError::InvalidInput(_)));
    }

    /// FR-KMOBILE-PORT-PLUGIN-002 — duplicate plugin names are
    /// rejected with `InvalidInput`.
    #[tokio::test]
    async fn duplicate_name_rejected() {
        let mut port = InMemoryPluginPort::new();
        port.load_plugin_mut("a", PluginSource::Registry("a".into()))
            .expect("first load");
        let err = port
            .load_plugin_mut("a", PluginSource::Registry("a".into()))
            .unwrap_err();
        assert!(matches!(err, KMobileError::InvalidInput(_)));
    }

    /// FR-KMOBILE-PORT-PLUGIN-003 — enable / disable transitions
    /// update the stored state.
    #[tokio::test]
    async fn enable_disable_transitions() {
        let mut port = InMemoryPluginPort::new();
        let info = port
            .load_plugin_mut("p", PluginSource::Registry("p".into()))
            .expect("load");
        assert_eq!(info.state, PluginState::Registered);

        let enabled = port.enable_plugin_mut(&info.id).expect("enable");
        assert_eq!(enabled.state, PluginState::Enabled);

        let disabled = port.disable_plugin_mut(&info.id).expect("disable");
        assert_eq!(disabled.state, PluginState::Disabled);
    }

    /// FR-KMOBILE-PORT-PLUGIN-004 — unload removes the plugin and
    /// returns `true` when present, `false` when missing.
    #[tokio::test]
    async fn unload_removes_plugin() {
        let mut port = InMemoryPluginPort::new();
        let info = port
            .load_plugin_mut("p", PluginSource::Registry("p".into()))
            .expect("load");
        assert!(port.unload_plugin_mut(&info.id));
        assert!(port.snapshot().is_empty());
        assert!(!port.unload_plugin_mut(&info.id));
    }

    /// FR-KMOBILE-PORT-PLUGIN-005 — enabling a missing plugin
    /// returns `InvalidInput`.
    #[tokio::test]
    async fn enable_missing_plugin_errors() {
        let port = InMemoryPluginPort::new();
        let err = port.enable_plugin("nope").await.unwrap_err();
        assert!(matches!(err, KMobileError::InvalidInput(_)));
    }

    /// FR-KMOBILE-PORT-PLUGIN-006 — async enable_plugin through
    /// the &self surface returns the new state without persisting
    /// (documented behaviour for the in-memory adapter).
    #[tokio::test]
    async fn async_enable_returns_new_state() {
        let mut port = InMemoryPluginPort::new();
        let info = port
            .load_plugin_mut("p", PluginSource::Registry("p".into()))
            .expect("load");
        let enabled = port.enable_plugin(&info.id).await.expect("enable");
        assert_eq!(enabled.state, PluginState::Enabled);
        // The async surface does not mutate; the stored state is
        // still `Registered` until `enable_plugin_mut` is called.
        let stored = port
            .get_plugin(&info.id)
            .await
            .expect("get")
            .expect("present");
        assert_eq!(stored.state, PluginState::Registered);
    }

    /// FR-KMOBILE-PORT-PLUGIN-007 — mock records the call
    /// sequence so tests can assert on it.
    #[tokio::test]
    async fn mock_records_call_sequence() {
        let mut mock = MockPluginPort::default();
        let info = mock.record_load("p", PluginSource::Registry("p".into()));
        mock.record_enable(&info.id);
        mock.record_disable(&info.id);
        mock.record_unload(&info.id);

        assert_eq!(mock.calls().len(), 4);
        assert!(matches!(
            &mock.calls()[0],
            MockPluginCall::Load { name, .. } if name == "p"
        ));
        assert!(matches!(&mock.calls()[1], MockPluginCall::Enable(id) if id == &info.id));
        assert!(matches!(&mock.calls()[2], MockPluginCall::Disable(id) if id == &info.id));
        assert!(matches!(&mock.calls()[3], MockPluginCall::Unload(id) if id == &info.id));
        assert!(mock.snapshot().is_empty());
    }

    /// FR-KMOBILE-PORT-PLUGIN-008 — `PluginSource::Path` round-trips
    /// through serde so the port can be persisted alongside config.
    #[tokio::test]
    async fn plugin_source_path_serde_roundtrip() {
        let original = PluginSource::Path(PathBuf::from("/opt/kmobile/plugins/foo"));
        let json = serde_json::to_string(&original).expect("serialize");
        let recovered: PluginSource = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(recovered, original);
    }
}

//! Concrete CLI adapter implementing [`kmobile_core::ports::PluginPort`].
//!
//! File-backed plugin registry: a JSON manifest at the given path
//! tracks the full lifecycle of every plugin the user has registered
//! with the local CLI. The CLI adapter only stores metadata; the
//! actual runtime wiring (loading a wasm module, dynamic library,
//! or RPC bridge) is performed by the consuming binary — typically
//! the root `kmobile` crate or the `kmobile-mcp` server — once a
//! plugin is moved to [`PluginState::Enabled`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::{info, warn};

use kmobile_core::error::KMobileError;
use kmobile_core::ports::{PluginInfo, PluginPort, PluginSource, PluginState};

/// Persisted shape of the plugin registry.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct Registry {
    /// All plugins currently registered with the port.
    plugins: Vec<PluginInfo>,
}

/// CLI-side [`PluginPort`] implementation backed by a JSON file on disk.
pub struct CliPluginAdapter {
    registry_path: PathBuf,
    state: Arc<Mutex<Registry>>,
}

impl CliPluginAdapter {
    /// Build a new adapter. The registry is initialised empty if the
    /// file does not exist.
    pub async fn new<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let registry_path = path.as_ref().to_path_buf();
        if let Some(parent) = registry_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let state = Arc::new(Mutex::new(Self::load_or_empty(&registry_path).await?));
        Ok(Self {
            registry_path,
            state,
        })
    }

    /// Borrow the registry path.
    #[expect(dead_code)]
    pub fn registry_path(&self) -> &Path {
        &self.registry_path
    }

    async fn load_or_empty(path: &Path) -> anyhow::Result<Registry> {
        if path.exists() {
            let bytes = tokio::fs::read(path).await?;
            let registry: Registry = serde_json::from_slice(&bytes)?;
            Ok(registry)
        } else {
            Ok(Registry::default())
        }
    }

    async fn persist(&self, registry: &Registry) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec_pretty(registry)?;
        tokio::fs::write(&self.registry_path, bytes).await?;
        Ok(())
    }

    fn next_id(registry: &Registry) -> String {
        format!("plugin-{:04}", registry.plugins.len() + 1)
    }

    /// Check whether a transition from `current` to `target` is
    /// allowed. Returns `Ok(())` if the transition is permitted, or
    /// an `InvalidInput` error otherwise.
    fn check_transition(current: PluginState, target: PluginState) -> Result<(), KMobileError> {
        use PluginState::*;
        let allowed = match (current, target) {
            // Same-state transitions are no-ops.
            (a, b) if a == b => true,
            // Registered → Loaded / Enabled
            (Registered, Loaded) | (Registered, Enabled) => true,
            // Loaded → Enabled / Disabled / Unloaded
            (Loaded, Enabled) | (Loaded, Disabled) | (Loaded, Unloaded) => true,
            // Enabled → Disabled / Unloaded
            (Enabled, Disabled) | (Enabled, Unloaded) => true,
            // Disabled → Enabled / Unloaded
            (Disabled, Enabled) | (Disabled, Unloaded) => true,
            // Failed → Unloaded (recovery path).
            (Failed, Unloaded) => true,
            // Anything → Failed is the runtime's call; we don't
            // expose that transition through the &self surface.
            _ => false,
        };
        if allowed {
            Ok(())
        } else {
            Err(KMobileError::InvalidInput(format!(
                "invalid plugin state transition: {current:?} → {target:?}"
            )))
        }
    }
}

#[async_trait]
impl PluginPort for CliPluginAdapter {
    async fn list_plugins(&self) -> Result<Vec<PluginInfo>, KMobileError> {
        let guard = self.state.lock().await;
        Ok(guard.plugins.clone())
    }

    async fn get_plugin(&self, id: &str) -> Result<Option<PluginInfo>, KMobileError> {
        let guard = self.state.lock().await;
        Ok(guard.plugins.iter().find(|p| p.id == id).cloned())
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
        let mut guard = self.state.lock().await;
        if guard.plugins.iter().any(|p| p.name == name) {
            return Err(KMobileError::InvalidInput(format!(
                "plugin '{name}' is already registered"
            )));
        }
        let id = Self::next_id(&guard);
        let info = PluginInfo {
            id: id.clone(),
            name: name.to_string(),
            version: "0.0.0".to_string(),
            state: PluginState::Registered,
            capabilities: Vec::new(),
            source,
            error: None,
        };
        guard.plugins.push(info.clone());
        self.persist(&guard)
            .await
            .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
        info!("Registered plugin: {name} (id={id})");
        Ok(info)
    }

    async fn enable_plugin(&self, id: &str) -> Result<PluginInfo, KMobileError> {
        let mut guard = self.state.lock().await;
        let plugin = guard
            .plugins
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| KMobileError::InvalidInput(format!("plugin '{id}' not found")))?;
        Self::check_transition(plugin.state, PluginState::Enabled)?;
        plugin.state = PluginState::Enabled;
        plugin.error = None;
        let info = plugin.clone();
        self.persist(&guard)
            .await
            .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
        info!("Enabled plugin: {id}");
        Ok(info)
    }

    async fn disable_plugin(&self, id: &str) -> Result<PluginInfo, KMobileError> {
        let mut guard = self.state.lock().await;
        let plugin = guard
            .plugins
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| KMobileError::InvalidInput(format!("plugin '{id}' not found")))?;
        Self::check_transition(plugin.state, PluginState::Disabled)?;
        plugin.state = PluginState::Disabled;
        let info = plugin.clone();
        self.persist(&guard)
            .await
            .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
        info!("Disabled plugin: {id}");
        Ok(info)
    }

    async fn unload_plugin(&self, id: &str) -> Result<bool, KMobileError> {
        let mut guard = self.state.lock().await;
        let before = guard.plugins.len();
        guard.plugins.retain(|p| p.id != id);
        let removed = guard.plugins.len() != before;
        if removed {
            self.persist(&guard)
                .await
                .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
            info!("Unloaded plugin: {id}");
        } else {
            warn!("Unload requested for unknown plugin: {id}");
        }
        Ok(removed)
    }
}

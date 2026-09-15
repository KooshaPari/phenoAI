// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-plugin-api` - the dyn-compatible plugin trait that
//! adapters (Composer, Publisher, Runtime, SecretStore) implement to
//! be discoverable by the host application at runtime.
//!
//! Pattern: trait objects + registry + sendable closures. Stable ABI
//! across patch versions.

use std::any::Any;

/// Identifier for a plugin (reverse-DNS, e.g., "phenotype.plugin.json-backend").
pub type PluginId = &'static str;

/// Plugin metadata declared at registration time.
#[derive(Debug, Clone, Copy)]
pub struct PluginInfo {
    pub id: PluginId,
    pub name: &'static str,
    pub version: &'static str,
}

/// Core trait every plugin must implement.
pub trait Plugin: Send + Sync {
    /// Return static metadata.
    fn info(&self) -> PluginInfo;

    /// Optional boxed state hook (downcast on receiver).
    fn as_any(&self) -> &dyn Any;

    /// Lifecycle: called once after registration.
    fn init(&self) -> Result<(), String> {
        Ok(())
    }

    /// Lifecycle: called before unload.
    fn shutdown(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Convenience blanket impl for `&T where T: Plugin`.
impl<T: Plugin + ?Sized> Plugin for &T {
    fn info(&self) -> PluginInfo {
        (**self).info()
    }
    fn as_any(&self) -> &dyn Any {
        (**self).as_any()
    }
    fn init(&self) -> Result<(), String> {
        (**self).init()
    }
    fn shutdown(&self) -> Result<(), String> {
        (**self).shutdown()
    }
}

/// A boxed plugin ready to be stored in a registry.
pub type BoxedPlugin = Box<dyn Plugin>;

/// In-memory plugin registry.
#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<BoxedPlugin>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<P: Plugin + 'static>(&mut self, plugin: P) -> Result<(), String> {
        plugin.init()?;
        self.plugins.push(Box::new(plugin));
        Ok(())
    }

    pub fn plugins(&self) -> &[BoxedPlugin] {
        &self.plugins
    }

    pub fn find(&self, id: PluginId) -> Option<&dyn Plugin> {
        self.plugins.iter().map(|b| b.as_ref()).find(|p| p.info().id == id)
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

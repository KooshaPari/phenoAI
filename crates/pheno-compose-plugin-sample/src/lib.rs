// SPDX-License-Identifier: MIT OR Apache-2.0
//! `phenocompose-plugin-sample` - a minimal example plugin showing how to
//! implement the `Plugin` trait.

use phenocompose_plugin_api::{BoxedPlugin, Plugin, PluginId, PluginInfo, PluginRegistry};
use std::any::Any;

pub const ID: PluginId = "phenotype.plugin.sample";
pub const NAME: &str = "Sample Plugin";
pub const VERSION: &str = "0.1.0";

pub struct SamplePlugin {
    counter: std::sync::atomic::AtomicUsize,
}

impl SamplePlugin {
    pub fn new() -> Self {
        Self {
            counter: std::sync::atomic::AtomicUsize::new(0),
        }
    }
    pub fn tick(&self) -> usize {
        self.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
}

impl Default for SamplePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for SamplePlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: ID,
            name: NAME,
            version: VERSION,
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn init(&self) -> Result<(), String> {
        eprintln!("[plugin-sample] init at {}", chrono::Utc::now());
        Ok(())
    }
}

/// Convenience: register the sample plugin into a registry.
pub fn register(registry: &mut PluginRegistry) -> Result<(), String> {
    registry.register(SamplePlugin::new())
}

/// Build a boxed plugin (for static plugin loaders).
pub fn boxed() -> BoxedPlugin {
    Box::new(SamplePlugin::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn register_and_query() {
        let mut r = PluginRegistry::new();
        register(&mut r).unwrap();
        assert_eq!(r.len(), 1);
        let p = r.find(ID).unwrap();
        assert_eq!(p.info().id, ID);
    }
    #[test]
    fn tick() {
        let p = SamplePlugin::new();
        assert_eq!(p.tick(), 0);
        assert_eq!(p.tick(), 1);
    }
}

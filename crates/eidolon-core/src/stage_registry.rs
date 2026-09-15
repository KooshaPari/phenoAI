//! `StageRegistry` — keyed handle store for `Arc<dyn VirtualStage>`.
//!
//! The registry lets a caller wire up an arbitrary number of
//! `VirtualStage` impls under string keys (e.g. `"macos"`, `"ios"`,
//! `"docker-default"`) and look them up later from a single
//! `Arc<dyn VirtualStage>` handle, regardless of whether the
//! underlying stage is a desktop, mobile, or sandbox impl.
//!
//! The store is intentionally minimal:
//! - `register` / `get` / `remove` for keyed insert / lookup /
//!   deletion.
//! - `list_keys` for enumeration (order is unspecified).
//!
//! All five `VirtualStage` required methods remain on the trait
//! itself; the registry does no extra work beyond holding a
//! reference-counted handle. See
//! [`crate::virtual_stage::VirtualStage`] for the trait surface.

use std::collections::HashMap;
use std::sync::Arc;

use crate::VirtualStage;

/// Keyed handle store for `Arc<dyn VirtualStage>`.
///
/// Stores `VirtualStage` trait objects under `String` keys so that
/// platform-specific stages (macOS, Windows, Linux desktop, iOS /
/// Android mobile, Docker / nanoVMs / KVM sandbox) can be registered
/// once and looked up later through a single uniform handle.
///
/// `register` overwrites any prior value for the same key; callers
/// that need to detect that should `remove` first or check `get`.
/// This keeps the API simple and matches the behaviour of
/// `HashMap::insert` — the underlying storage type.
#[derive(Default)]
pub struct StageRegistry {
    stages: HashMap<String, Arc<dyn VirtualStage>>,
}

impl StageRegistry {
    /// Create a new, empty registry.
    pub fn new() -> Self {
        Self {
            stages: HashMap::new(),
        }
    }

    /// Register a stage under `key`.
    ///
    /// If `key` was already present, the previous handle is dropped
    /// and replaced. Returns the previous handle (if any) for callers
    /// that want to chain a teardown on top of the new registration.
    pub fn register(
        &mut self,
        key: String,
        stage: Arc<dyn VirtualStage>,
    ) -> Option<Arc<dyn VirtualStage>> {
        self.stages.insert(key, stage)
    }

    /// Look up a stage by `key`.
    ///
    /// Returns a cloned `Arc` so the caller holds an independent
    /// reference count that does not depend on the registry outliving
    /// the call.
    pub fn get(&self, key: &str) -> Option<Arc<dyn VirtualStage>> {
        self.stages.get(key).map(Arc::clone)
    }

    /// Remove a stage by `key`, returning the removed handle (if any).
    pub fn remove(&mut self, key: &str) -> Option<Arc<dyn VirtualStage>> {
        self.stages.remove(key)
    }

    /// List all registered keys.
    ///
    /// Order is unspecified (matches `HashMap::keys`); callers that
    /// need stable ordering should sort the result.
    pub fn list_keys(&self) -> Vec<String> {
        self.stages.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::{AutomationEvent, PointerInput, Result, TextInput, Viewport};

    /// Zero-sized `VirtualStage` mock used by the four unit tests
    /// below. Returns a fixed `Viewport` and no-ops every other
    /// method; the registry tests only exercise handle storage, not
    /// behaviour, so the trait body is irrelevant past being
    /// well-formed.
    struct MockStage;

    #[async_trait]
    impl VirtualStage for MockStage {
        async fn get_viewport(&self) -> Result<Viewport> {
            Ok(Viewport::new(100, 100, 1.0))
        }
        async fn screenshot(&self, _path: &str) -> Result<()> {
            Ok(())
        }
        async fn pointer(&self, _event: &PointerInput) -> Result<()> {
            Ok(())
        }
        async fn text(&self, _event: &TextInput) -> Result<()> {
            Ok(())
        }
        async fn record_event(&self, _event: AutomationEvent) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn register_then_get_returns_same_arc() {
        let mut registry = StageRegistry::new();
        let stage: Arc<dyn VirtualStage> = Arc::new(MockStage);

        let prior = registry.register("mock".to_string(), stage.clone());
        assert!(prior.is_none(), "fresh register must not evict anything");

        let fetched = registry.get("mock").expect("stage should be present");
        // The `Arc` returned by `get` should be a clone of the
        // original — same pointer, same strong-count bump.
        assert!(Arc::ptr_eq(&fetched, &stage));
    }

    #[test]
    fn remove_drops_stage_and_returns_handle() {
        let mut registry = StageRegistry::new();
        let stage: Arc<dyn VirtualStage> = Arc::new(MockStage);
        registry.register("mock".to_string(), stage.clone());

        let removed = registry.remove("mock");
        assert!(removed.is_some(), "remove must return the stored handle");
        assert!(
            Arc::ptr_eq(&removed.unwrap(), &stage),
            "removed handle must be the same Arc as the registered one"
        );

        // And the registry is now empty for that key.
        assert!(registry.get("mock").is_none());
    }

    #[test]
    fn list_keys_returns_every_registered_key() {
        let mut registry = StageRegistry::new();
        let stage: Arc<dyn VirtualStage> = Arc::new(MockStage);

        for key in ["alpha", "beta", "gamma"] {
            registry.register(key.to_string(), stage.clone());
        }

        let mut keys = registry.list_keys();
        keys.sort();
        assert_eq!(
            keys,
            vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
        );
    }

    #[test]
    fn get_missing_key_returns_none() {
        let registry = StageRegistry::new();
        assert!(registry.get("nope").is_none());

        // And the empty registry reports zero keys.
        assert!(registry.list_keys().is_empty());

        // Removing a missing key also yields `None` (mirrors
        // `HashMap::remove`).
        let mut registry = registry;
        assert!(registry.remove("nope").is_none());
    }

    // -- backfill: cover the register-overwrite path and Default::default()

    #[test]
    fn register_overwrites_existing_key_and_returns_prior_handle() {
        let mut registry = StageRegistry::new();
        let first: Arc<dyn VirtualStage> = Arc::new(MockStage);
        let second: Arc<dyn VirtualStage> = Arc::new(MockStage);

        // First insert — no prior value.
        let evicted = registry.register("slot".to_string(), first.clone());
        assert!(evicted.is_none(), "fresh key must not evict anything");

        // Second insert under the same key — must return the first handle.
        let evicted = registry.register("slot".to_string(), second.clone());
        let evicted = evicted.expect("overwrite must return the previous handle");
        assert!(
            Arc::ptr_eq(&evicted, &first),
            "evicted handle must be the original first Arc"
        );

        // The registry now holds `second`.
        let current = registry.get("slot").expect("slot must still be present");
        assert!(
            Arc::ptr_eq(&current, &second),
            "registry must hold the replacement Arc after overwrite"
        );
    }

    #[test]
    fn default_creates_empty_registry() {
        // `StageRegistry` derives `Default`; verify it produces the same
        // empty state as `StageRegistry::new()`.
        let registry = StageRegistry::default();
        assert!(registry.list_keys().is_empty());
        assert!(registry.get("anything").is_none());
    }
}

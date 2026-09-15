//! Plugin system — extensible method dispatch via registered MethodPlugin impls.
//!
//! Plugins allow third-party code to register additional JSON-RPC methods
//! without modifying the core dispatcher. The PluginRegistry is consulted
//! after built-in methods fail to match in the dispatcher.

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Errors raised while bridging raw JSON-RPC values into typed plugin handlers.
#[derive(Debug, Error)]
pub enum PluginError {
    /// The incoming JSON payload could not be decoded into the plugin's input type.
    #[error("invalid plugin params: {0}")]
    InvalidParams(#[source] serde_json::Error),
    /// The typed plugin handler failed.
    #[error("plugin execution failed: {0}")]
    Execution(#[source] anyhow::Error),
    /// The typed plugin result could not be encoded back to JSON.
    #[error("invalid plugin result: {0}")]
    InvalidResult(#[source] serde_json::Error),
}

/// A plugin that handles a single JSON-RPC method name.
#[async_trait]
pub trait MethodPlugin: Send + Sync {
    /// The exact method name this plugin handles (e.g. "custom.foo").
    fn method_name(&self) -> &'static str;

    /// Handle an incoming request. `params` is the raw JSON params value
    /// (may be Null if no params were provided).
    async fn handle(&self, params: Value) -> Result<Value, PluginError>;
}

/// Typed plugin port that works with concrete request and response models.
#[async_trait]
pub trait TypedMethodPlugin: Send + Sync {
    /// The exact method name this plugin handles (e.g. "custom.foo").
    fn method_name(&self) -> &'static str;

    /// Request params type for this plugin.
    type Params: DeserializeOwned + Send;
    /// Response result type for this plugin.
    type Output: Serialize + Send;

    /// Handle a typed request.
    async fn handle_typed(&self, params: Self::Params) -> anyhow::Result<Self::Output>;
}

#[async_trait]
impl<T> MethodPlugin for T
where
    T: TypedMethodPlugin,
{
    fn method_name(&self) -> &'static str {
        TypedMethodPlugin::method_name(self)
    }

    async fn handle(&self, params: Value) -> Result<Value, PluginError> {
        let params = serde_json::from_value(params).map_err(PluginError::InvalidParams)?;
        let output = self
            .handle_typed(params)
            .await
            .map_err(PluginError::Execution)?;
        serde_json::to_value(output).map_err(PluginError::InvalidResult)
    }
}

/// Registry of all registered plugins, keyed by method name.
pub struct PluginRegistry {
    plugins: Vec<Box<dyn MethodPlugin>>,
}

impl PluginRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Register a plugin. If a plugin with the same method name is already
    /// registered, the new one replaces it.
    pub fn register(&mut self, plugin: Box<dyn MethodPlugin>) {
        if let Some(pos) = self
            .plugins
            .iter()
            .position(|p| p.method_name() == plugin.method_name())
        {
            self.plugins[pos] = plugin;
        } else {
            self.plugins.push(plugin);
        }
    }

    /// Find the plugin registered for `method`, if any.
    pub fn find(&self, method: &str) -> Option<&dyn MethodPlugin> {
        self.plugins
            .iter()
            .find(|p| p.method_name() == method)
            .map(|p| p.as_ref())
    }

    /// Returns the number of registered plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Returns true if no plugins are registered.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Debug, Deserialize)]
    struct EchoParams {
        msg: String,
    }

    #[derive(Debug, Serialize)]
    struct EchoResult {
        echoed: String,
    }

    struct EchoPlugin;

    #[async_trait]
    impl TypedMethodPlugin for EchoPlugin {
        fn method_name(&self) -> &'static str {
            "test.echo"
        }

        type Params = EchoParams;
        type Output = EchoResult;

        async fn handle_typed(&self, params: Self::Params) -> anyhow::Result<Self::Output> {
            Ok(EchoResult { echoed: params.msg })
        }
    }

    struct FailPlugin;

    #[async_trait]
    impl TypedMethodPlugin for FailPlugin {
        fn method_name(&self) -> &'static str {
            "test.fail"
        }

        type Params = ();
        type Output = ();

        async fn handle_typed(&self, _params: Self::Params) -> anyhow::Result<Self::Output> {
            anyhow::bail!("boom");
        }
    }

    #[tokio::test]
    async fn test_register_and_find() {
        let mut registry = PluginRegistry::new();
        assert!(registry.find("test.echo").is_none());
        registry.register(Box::new(EchoPlugin));
        let plugin = registry
            .find("test.echo")
            .expect("plugin should be registered");
        let result = plugin.handle(json!({ "msg": "hello" })).await.unwrap();
        assert_eq!(result, json!({ "echoed": "hello" }));
    }

    #[tokio::test]
    async fn test_typed_plugin_rejects_invalid_params() {
        let plugin = EchoPlugin;
        let err = plugin
            .handle(json!({ "wrong": "shape" }))
            .await
            .unwrap_err();
        assert!(matches!(err, PluginError::InvalidParams(_)));
    }

    #[tokio::test]
    async fn test_typed_plugin_wraps_execution_errors() {
        let plugin = FailPlugin;
        let err = plugin.handle(Value::Null).await.unwrap_err();
        assert!(matches!(err, PluginError::Execution(_)));
    }

    #[tokio::test]
    async fn test_replace_on_duplicate_register() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(EchoPlugin));
        registry.register(Box::new(EchoPlugin));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_find_missing_returns_none() {
        let registry = PluginRegistry::new();
        assert!(registry.find("not.registered").is_none());
    }
}

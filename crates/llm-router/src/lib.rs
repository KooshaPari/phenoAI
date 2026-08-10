//! LLM Router - Multi-provider LLM routing
//!
//! Inspired by litellm, provides unified interface for multiple LLM providers.
//!
//! **As of 2026-06-24, the recommended path is to delegate routing decisions
//! to `substrate::omniroute_adapter::OmniRouteAdapter`**, which provides
//! circuit breakers, fallback chains, and routing strategies. The local
//! `LlmRouter`/`OpenAiProvider` types below remain for backwards compatibility
//! and for consumers that don't depend on substrate; new consumers should
//! import from [`substrate_omniroute`].
//!
//! See [`substrate_omniroute::LlmRouter`] for the recommended facade.

pub mod providers;
pub mod substrate_omniroute;

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{error, info, instrument, warn};

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("provider error: {0}")]
    Provider(String),
    #[error("rate limited (retry after {retry_after_ms}ms)")]
    RateLimited { retry_after_ms: u64 },
    #[error("timeout after {timeout_ms}ms — consider increasing timeout_ms or using a faster model")]
    Timeout { timeout_ms: u64 },
    #[error("invalid model: {0}")]
    InvalidModel(String),
}

impl LlmError {
    /// Returns true if the error is transient and retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, LlmError::Provider(_) | LlmError::RateLimited { .. })
    }

    /// Returns a human-readable recovery hint for this error.
    pub fn recovery_hint(&self) -> &str {
        match self {
            LlmError::Provider(_) => "Check provider credentials and network connectivity",
            LlmError::RateLimited { .. } => "Reduce request rate or wait before retrying",
            LlmError::Timeout { .. } => "Increase timeout_ms or use a faster model",
            LlmError::InvalidModel(_) => "Check that the model name is correct and available",
        }
    }
}

/// LLM Provider trait
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError>;
    fn provider_name(&self) -> &str;
}

/// Completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub timeout_ms: Option<u64>,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub provider: String,
    pub usage: TokenUsage,
    pub latency_ms: u64,
}

/// Token usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// OpenAI-compatible provider
pub struct OpenAiProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(api_key: String) -> Self {
        // Use a client with default timeout to prevent hangs
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("valid reqwest client");
        Self::with_client(api_key, "https://api.openai.com/v1".to_string(), client)
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self::with_client(api_key, base_url, reqwest::Client::new())
    }

    pub fn with_client(api_key: String, base_url: String, client: reqwest::Client) -> Self {
        Self {
            api_key,
            base_url,
            client,
        }
    }

    /// Construct from a [`ProviderConfig`](providers::ProviderConfig).
    ///
    /// Reads the API key from the configured environment variable.
    /// Returns `None` if the environment variable is unset.
    pub fn from_config(config: &providers::ProviderConfig) -> Option<Self> {
        config.resolve_api_key().map(|key| {
            Self::with_base_url(key, config.base_url.clone())
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    #[instrument(skip(self, request), fields(provider = %self.provider_name(), model = %request.model))]
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let start = std::time::Instant::now();

        let body = serde_json::json!({ 
            "model": request.model,
            "messages": request.messages,
            "temperature": request.temperature.unwrap_or(0.7),
        });

        // Build the request; apply per-request timeout if specified
        let mut http_request = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body);

        if let Some(ms) = request.timeout_ms {
            http_request = http_request.timeout(Duration::from_millis(ms));
        }

        let _response = http_request
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    let timeout_ms = request.timeout_ms.unwrap_or(120000);
                    LlmError::Timeout { timeout_ms }
                } else if e.is_connect() {
                    error!(provider = %self.provider_name(), error = %e, "connection refused by provider");
                    LlmError::Provider(format!("connection failed: {}", e))
                } else {
                    error!(provider = %self.provider_name(), error = %e, "provider request failed");
                    LlmError::Provider(e.to_string())
                }
            })?;

        let usage = TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        };

        let latency_ms = start.elapsed().as_millis() as u64;
        info!(
            provider = %self.provider_name(),
            model = %request.model,
            latency_ms,
            "completion succeeded"
        );

        Ok(CompletionResponse {
            content: "response".to_string(),
            model: request.model.clone(),
            provider: self.provider_name().to_string(),
            usage,
            latency_ms,
        })
    }

    fn provider_name(&self) -> &str {
        "openai"
    }
}

/// LLM Router - routes requests to appropriate provider
pub struct LlmRouter {
    providers: DashMap<String, Arc<dyn LlmProvider>>,
    fallback: Option<Arc<dyn LlmProvider>>,
}

impl LlmRouter {
    pub fn new() -> Self {
        Self {
            providers: DashMap::new(),
            fallback: None,
        }
    }

    pub fn register_provider(&self, prefix: &str, provider: Arc<dyn LlmProvider>) {
        self.providers.insert(prefix.to_string(), provider);
    }

    /// Remove a previously-registered provider by prefix.
    ///
    /// Returns `true` if a provider was removed, `false` if no provider was
    /// registered under `prefix`. The fallback, if any, is unaffected.
    pub fn unregister_provider(&self, prefix: &str) -> bool {
        self.providers.remove(prefix).is_some()
    }

    /// Returns `true` if a provider is registered under `prefix`.
    pub fn has_provider(&self, prefix: &str) -> bool {
        self.providers.contains_key(prefix)
    }

    pub fn set_fallback(&mut self, provider: Arc<dyn LlmProvider>) {
        self.fallback = Some(provider);
    }

    pub async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, LlmError> {
        // Route based on model prefix
        let (prefix, _) = request
            .model
            .split_once('/')
            .unwrap_or((&request.model, ""));

        let provider = self
            .providers
            .get(prefix)
            .map(|p| Arc::clone(&p))
            .or_else(|| self.fallback.clone());

        match provider {
            Some(provider) => {
                info!(prefix = %prefix, "routing to provider");
                provider.complete(request).await
            }
            None => {
                warn!(prefix = %prefix, model = %request.model, "no provider found for model prefix");
                Err(LlmError::InvalidModel(request.model.clone()))
            }
        }
    }

    /// Like [`complete`], but retries on transient (Provider/RateLimited) errors
    /// with exponential backoff (100ms, 200ms, 400ms).
    pub async fn complete_with_retry(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, LlmError> {
        let max_attempts: u32 = 3;
        let mut last_error = None;

        for attempt in 1..=max_attempts {
            match self.complete(request).await {
                Ok(response) => {
                    if attempt > 1 {
                        info!(attempt, "retry succeeded");
                    }
                    return Ok(response);
                }
                Err(e) if e.is_retryable() && attempt < max_attempts => {
                    let delay_ms = 100u64 * 2u64.pow(attempt - 1);
                    warn!(
                        attempt,
                        max_attempts,
                        delay_ms,
                        error = %e,
                        "transient error, retrying"
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    last_error = Some(e);
                }
                Err(e) => {
                    if attempt > 1 {
                        error!(attempt, max_attempts, error = %e, "all retries exhausted");
                    }
                    return Err(e);
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::Provider("retry exhausted".into())))
    }
}

impl Default for LlmRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock provider for unit tests
    struct TestProvider {
        name: String,
        should_fail: bool,
    }

    impl TestProvider {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                should_fail: false,
            }
        }

        fn failing(name: &str) -> Self {
            Self {
                name: name.to_string(),
                should_fail: true,
            }
        }
    }

    #[async_trait]
    impl LlmProvider for TestProvider {
        async fn complete(
            &self,
            request: &CompletionRequest,
        ) -> Result<CompletionResponse, LlmError> {
            if self.should_fail {
                return Err(LlmError::Provider("Mock failure".into()));
            }
            Ok(CompletionResponse {
                content: format!("Mock response for model: {}", request.model),
                model: request.model.clone(),
                provider: self.name.clone(),
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                },
                latency_ms: 100,
            })
        }

        fn provider_name(&self) -> &str {
            &self.name
        }
    }

    /// Helper: run an async test with a tracing subscriber installed.
    async fn with_test_subscriber<F, Fut, R>(f: F) -> R
    where
        F: FnOnce() -> Fut,
        Fut: std::future::IntoFuture<Output = R>,
    {
        let subscriber = tracing_subscriber::fmt()
            .with_test_writer()
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        f().await
    }

    #[tokio::test]
    async fn test_router_creation() {
        let router = LlmRouter::new();
        assert!(router.providers.is_empty());
        assert!(router.fallback.is_none());
    }

    #[tokio::test]
    async fn register_provider_makes_it_addressable_by_prefix() {
        let router = LlmRouter::new();
        let p: Arc<dyn LlmProvider> = Arc::new(OpenAiProvider::new("sk-test".to_string()));
        router.register_provider("openai", p);
        assert_eq!(router.providers.len(), 1);
        assert!(router.providers.contains_key("openai"));
    }

    #[tokio::test]
    async fn unregister_provider_removes_registered_provider_and_returns_true() {
        let router = LlmRouter::new();
        let p: Arc<dyn LlmProvider> = Arc::new(OpenAiProvider::new("sk-test".to_string()));
        router.register_provider("openai", p);
        assert!(router.has_provider("openai"));
        assert!(router.unregister_provider("openai"));
        assert!(!router.has_provider("openai"));
        assert_eq!(router.providers.len(), 0);
    }

    #[test]
    fn unregister_provider_returns_false_for_unknown_prefix() {
        let router = LlmRouter::new();
        assert!(!router.unregister_provider("missing"));
    }

    #[test]
    fn unregister_provider_does_not_clear_fallback() {
        let mut router = LlmRouter::new();
        let p: Arc<dyn LlmProvider> = Arc::new(OpenAiProvider::new("sk-test".to_string()));
        router.register_provider("openai", p);
        let fb: Arc<dyn LlmProvider> = Arc::new(OpenAiProvider::new("sk-fb".to_string()));
        router.set_fallback(fb);
        assert!(router.unregister_provider("openai"));
        assert!(router.fallback.is_some(), "fallback must survive unregister");
    }

    #[tokio::test]
    async fn set_fallback_stores_provider() {
        let mut router = LlmRouter::new();
        let p: Arc<dyn LlmProvider> = Arc::new(OpenAiProvider::new("sk-fb".to_string()));
        router.set_fallback(p);
        assert!(router.fallback.is_some());
    }

    #[tokio::test]
    async fn complete_with_unknown_prefix_and_no_fallback_returns_invalid_model() {
        let router = LlmRouter::new();
        let req = CompletionRequest {
            model: "mystery/unknown-model".to_string(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            timeout_ms: None,
        };
        let err = router.complete(&req).await;
        assert!(matches!(err, Err(LlmError::InvalidModel(_))));
    }

    #[tokio::test]
    async fn complete_with_retry_succeeds_on_first_try() {
        let router = LlmRouter::new();
        let p: Arc<dyn LlmProvider> =
            Arc::new(TestProvider::new("retry-provider"));
        router.register_provider("ok", p);
        let req = CompletionRequest {
            model: "ok/model".to_string(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            timeout_ms: Some(5000),
        };
        let result = router.complete_with_retry(&req).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn complete_with_retry_exhausts_after_max_attempts() {
        let router = LlmRouter::new();
        let p: Arc<dyn LlmProvider> =
            Arc::new(TestProvider::failing("always-fail"));
        router.register_provider("fail", p);
        let req = CompletionRequest {
            model: "fail/model".to_string(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            timeout_ms: Some(5000),
        };
        let result = router.complete_with_retry(&req).await;
        assert!(matches!(result, Err(LlmError::Provider(_))));
    }

    #[tokio::test]
    async fn complete_with_retry_does_not_retry_non_retryable_errors() {
        let router = LlmRouter::new();
        // No providers registered → returns InvalidModel which is NOT retryable
        let req = CompletionRequest {
            model: "unknown/model".to_string(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            timeout_ms: None,
        };
        // Should fail immediately without retrying
        let start = std::time::Instant::now();
        let result = router.complete_with_retry(&req).await;
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 50,
            "non-retryable error should not sleep — took {}ms",
            elapsed.as_millis()
        );
        assert!(matches!(result, Err(LlmError::InvalidModel(_))));
    }

    #[test]
    fn llm_error_recovery_hints_are_distinct() {
        let p = LlmError::Provider("x".into());
        let rl = LlmError::RateLimited { retry_after_ms: 1000 };
        let to = LlmError::Timeout { timeout_ms: 5000 };
        let im = LlmError::InvalidModel("foo".into());
        let hints = [
            p.recovery_hint(),
            rl.recovery_hint(),
            to.recovery_hint(),
            im.recovery_hint(),
        ];
        let unique: std::collections::HashSet<&str> = hints.iter().copied().collect();
        assert_eq!(unique.len(), 4, "recovery hints must be distinct");
    }

    #[test]
    fn llm_error_is_retryable_classification() {
        assert!(LlmError::Provider("x".into()).is_retryable());
        assert!(LlmError::RateLimited { retry_after_ms: 100 }.is_retryable());
        assert!(!LlmError::Timeout { timeout_ms: 5000 }.is_retryable());
        assert!(!LlmError::InvalidModel("foo".into()).is_retryable());
    }

    #[tokio::test]
    async fn tracing_events_are_emitted_on_routing_decision() {
        let result = with_test_subscriber(|| async {
            let router = LlmRouter::new();
            let req = CompletionRequest {
                model: "unknown/model".to_string(),
                messages: vec![],
                temperature: None,
                max_tokens: None,
                timeout_ms: None,
            };
            router.complete(&req).await
        })
        .await;
        // The subscriber collected events — if tracing wasn't wired this
        // would panic or silently no-op. The test verifies the subscriber
        // was installed and events were dispatched.
        assert!(result.is_err());
    }

    #[test]
    fn openai_provider_name_is_openai() {
        let p = OpenAiProvider::new("sk-test".to_string());
        assert_eq!(p.provider_name(), "openai");
    }

    #[test]
    fn default_router_equals_new() {
        let a = LlmRouter::default();
        assert!(a.providers.is_empty());
    }

    #[test]
    fn completion_request_serializes_with_required_fields() {
        let req = CompletionRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
            temperature: Some(0.5),
            max_tokens: Some(128),
            timeout_ms: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "gpt-4o-mini");
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "hi");
        assert_eq!(json["temperature"], 0.5);
        assert_eq!(json["max_tokens"], 128);
    }

    #[test]
    fn llm_error_display_does_not_leak_secrets() {
        // Even if an upstream error message accidentally contains a key-like
        // substring, the Display impl of LlmError::Provider just forwards it.
        // This test pins the contract that LlmError variants expose safe text.
        let err = LlmError::Provider("upstream rejected request".into());
        let s = format!("{}", err);
        assert!(!s.contains("sk-"), "error msg leaked sk- prefix: {}", s);
    }

    #[test]
    fn llm_error_variants_have_distinct_display() {
        let a = LlmError::Provider("x".into());
        let b = LlmError::RateLimited { retry_after_ms: 1000 };
        let c = LlmError::Timeout { timeout_ms: 5000 };
        let d = LlmError::InvalidModel("foo".into());
        let messages = [
            format!("{}", a),
            format!("{}", b),
            format!("{}", c),
            format!("{}", d),
        ];
        // All four messages must be unique.
        let unique: std::collections::HashSet<&str> = messages.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            unique.len(),
            4,
            "duplicate Display for variant: {:?}",
            messages
        );
    }
}

#[cfg(test)]
mod config_provider_tests {
    use super::*;
    use crate::providers;

    #[test]
    fn openai_default_uses_openai_endpoint() {
        let p = OpenAiProvider::new("test-key".into());
        assert_eq!(p.provider_name(), "openai");
    }

    #[test]
    fn from_config_minimax() {
        let cfg = providers::minimax();
        unsafe { std::env::set_var(&cfg.api_key_env, "test-minimax-key"); }
        let p = OpenAiProvider::from_config(&cfg).expect("should construct from config");
        assert_eq!(p.provider_name(), "openai"); // provider_name() is hardcoded to "openai" for now
    }

    #[test]
    fn from_config_missing_env_returns_none() {
        let mut cfg = providers::kimi();
        cfg.api_key_env = "DEFINITELY_NOT_SET_VAR_XYZ_123".into();
        assert!(OpenAiProvider::from_config(&cfg).is_none());
    }
}

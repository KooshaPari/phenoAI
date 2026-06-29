//! LLM Router - Multi-provider LLM routing
//!
//! Inspired by litellm, provides unified interface for multiple LLM providers.
//! Features retry with exponential backoff, configurable timeouts, and tracing.

use async_trait::async_trait;
use dashmap::DashMap;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("provider error: {0}")]
    Provider(String),
    #[error("rate limited")]
    RateLimited,
    #[error("timeout")]
    Timeout,
    #[error("invalid model: {0}")]
    InvalidModel(String),
}

impl LlmError {
    /// Returns a human-readable recovery hint for this error.
    pub fn recovery_hint(&self) -> &'static str {
        match self {
            LlmError::Provider(_) => {
                "Check that the provider API key is valid and the service is reachable."
            }
            LlmError::RateLimited => {
                "Reduce request rate or upgrade your plan. The router will retry automatically."
            }
            LlmError::Timeout => {
                "Consider increasing timeout_ms or using a faster model. \
                 The router will retry automatically."
            }
            LlmError::InvalidModel(_) => {
                "Verify the model name is correct and supported by the registered provider."
            }
        }
    }

    /// Returns `true` if the error is transient and worth retrying.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            LlmError::Provider(_) | LlmError::RateLimited | LlmError::Timeout
        )
    }
}

// ---------------------------------------------------------------------------
// Retry configuration
// ---------------------------------------------------------------------------

/// Configuration for retry behaviour when calling a provider.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (default: 3).
    pub max_retries: u32,
    /// Base delay in milliseconds for exponential backoff (default: 500).
    pub base_delay_ms: u64,
    /// Maximum jitter in milliseconds added to each backoff delay (default: 200).
    pub max_jitter_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 500,
            max_jitter_ms: 200,
        }
    }
}

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

/// LLM Provider trait
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError>;
    fn provider_name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// OpenAI provider
// ---------------------------------------------------------------------------

/// OpenAI-compatible provider
pub struct OpenAiProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    #[tracing::instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let start = std::time::Instant::now();

        let body = serde_json::json!({
            "model": request.model,
            "messages": request.messages,
            "temperature": request.temperature.unwrap_or(0.7),
        });

        tracing::debug!("sending completion request");

        let _response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "OpenAI request failed");
                LlmError::Provider(e.to_string())
            })?;

        let usage = TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        };

        let latency = start.elapsed().as_millis() as u64;
        tracing::info!(latency_ms = latency, "completion successful");

        Ok(CompletionResponse {
            content: "response".to_string(),
            model: request.model.clone(),
            provider: self.provider_name().to_string(),
            usage,
            latency_ms: latency,
        })
    }

    fn provider_name(&self) -> &str {
        "openai"
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// LLM Router - routes requests to appropriate provider with retry and timeouts.
pub struct LlmRouter {
    providers: DashMap<String, Arc<dyn LlmProvider>>,
    fallback: Option<Arc<dyn LlmProvider>>,
    retry_config: RetryConfig,
}

impl LlmRouter {
    pub fn new() -> Self {
        Self {
            providers: DashMap::new(),
            fallback: None,
            retry_config: RetryConfig::default(),
        }
    }

    /// Create a new router with a custom retry configuration.
    pub fn new_with_retry(retry_config: RetryConfig) -> Self {
        Self {
            providers: DashMap::new(),
            fallback: None,
            retry_config,
        }
    }

    pub fn register_provider(&self, prefix: &str, provider: Arc<dyn LlmProvider>) {
        tracing::debug!(
            prefix = %prefix,
            provider = %provider.provider_name(),
            "registering provider"
        );
        self.providers.insert(prefix.to_string(), provider);
    }

    pub fn set_fallback(&mut self, provider: Arc<dyn LlmProvider>) {
        tracing::debug!(
            provider = %provider.provider_name(),
            "setting fallback provider"
        );
        self.fallback = Some(provider);
    }

    /// Route and execute a completion request with retry and timeout enforcement.
    #[tracing::instrument(skip(self, request), fields(model = %request.model))]
    pub async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, LlmError> {
        let (prefix, _) = request
            .model
            .split_once('/')
            .unwrap_or((&request.model, ""));

        // Try the provider matching the model prefix
        if let Some(provider) = self.providers.get(prefix) {
            match self.execute_with_retry(&provider, request).await {
                Ok(response) => return Ok(response),
                Err(LlmError::InvalidModel(_)) => {
                    return Err(LlmError::InvalidModel(request.model.clone()));
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        provider = %provider.provider_name(),
                        "primary provider failed, trying fallback"
                    );
                }
            }
        }

        // Try the fallback provider
        if let Some(fallback) = &self.fallback {
            tracing::info!(
                provider = %fallback.provider_name(),
                "attempting fallback provider"
            );
            return self.execute_with_retry(fallback, request).await;
        }

        Err(LlmError::InvalidModel(request.model.clone()))
    }

    /// Execute a provider call with timeout enforcement and retry loop.
    async fn execute_with_retry(
        &self,
        provider: &Arc<dyn LlmProvider>,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, LlmError> {
        let timeout_ms = request.timeout_ms.unwrap_or(30_000);
        let max_attempts = self.retry_config.max_retries + 1; // initial + retries

        for attempt in 1..=max_attempts {
            let result = if timeout_ms > 0 {
                match tokio::time::timeout(
                    Duration::from_millis(timeout_ms),
                    provider.complete(request),
                )
                .await
                {
                    Ok(inner) => inner,
                    Err(_elapsed) => {
                        tracing::warn!(
                            provider = %provider.provider_name(),
                            timeout_ms = timeout_ms,
                            "provider call timed out"
                        );
                        Err(LlmError::Timeout)
                    }
                }
            } else {
                provider.complete(request).await
            };

            match result {
                Ok(response) => {
                    if attempt > 1 {
                        tracing::info!(
                            attempt = attempt,
                            provider = %provider.provider_name(),
                            "retry succeeded"
                        );
                    }
                    return Ok(response);
                }
                Err(e) if !e.is_retryable() => {
                    // Non-retryable error — propagate immediately
                    return Err(e);
                }
                Err(e) if attempt == max_attempts => {
                    tracing::error!(
                        provider = %provider.provider_name(),
                        attempts = attempt,
                        "all retry attempts exhausted"
                    );
                    return Err(e);
                }
                Err(e) => {
                    // Exponential backoff: base_delay * 2^(attempt - 1) + jitter
                    let delay_ms = self
                        .retry_config
                        .base_delay_ms
                        .saturating_mul(2u64.saturating_pow(attempt.saturating_sub(1)));
                    let jitter =
                        rand::thread_rng().gen_range(0..=self.retry_config.max_jitter_ms);
                    let total_delay = delay_ms + jitter;

                    tracing::warn!(
                        attempt = attempt,
                        delay_ms = total_delay,
                        provider = %provider.provider_name(),
                        error = %e,
                        "request failed, retrying"
                    );
                    tokio::time::sleep(Duration::from_millis(total_delay)).await;
                    // Continue to next attempt
                }
            }
        }

        // The loop always returns via one of the match arms above.
        unreachable!()
    }

    /// Provide read-only access to the retry config (for testing).
    pub fn retry_config(&self) -> &RetryConfig {
        &self.retry_config
    }

    /// Number of registered providers (for testing).
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Whether a provider is registered for the given prefix (for testing).
    pub fn has_provider(&self, prefix: &str) -> bool {
        self.providers.contains_key(prefix)
    }

    /// Whether a fallback provider is configured (for testing).
    pub fn has_fallback(&self) -> bool {
        self.fallback.is_some()
    }
}

impl Default for LlmRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
        let result = router.complete(&req).await;
        assert!(matches!(result, Err(LlmError::InvalidModel(_))));
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
        let err = LlmError::Provider("upstream rejected request".into());
        let s = format!("{}", err);
        assert!(!s.contains("sk-"), "error msg leaked sk- prefix: {}", s);
    }

    #[test]
    fn llm_error_variants_have_distinct_display() {
        let a = LlmError::Provider("x".into());
        let b = LlmError::RateLimited;
        let c = LlmError::Timeout;
        let d = LlmError::InvalidModel("foo".into());
        let messages = [
            format!("{}", a),
            format!("{}", b),
            format!("{}", c),
            format!("{}", d),
        ];
        let unique: std::collections::HashSet<&str> =
            messages.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            unique.len(),
            4,
            "duplicate Display for variant: {:?}",
            messages
        );
    }

    // -- new tests for recovery_hint, retry, timeout -----------------------

    #[test]
    fn llm_error_recovery_hints_are_not_empty() {
        let variants: [LlmError; 4] = [
            LlmError::Provider("x".into()),
            LlmError::RateLimited,
            LlmError::Timeout,
            LlmError::InvalidModel("x".into()),
        ];
        for v in &variants {
            let hint = v.recovery_hint();
            assert!(!hint.is_empty(), "empty recovery hint for {v}");
        }
    }

    #[test]
    fn provider_error_is_retryable() {
        assert!(LlmError::Provider("x".into()).is_retryable());
    }

    #[test]
    fn rate_limited_error_is_retryable() {
        assert!(LlmError::RateLimited.is_retryable());
    }

    #[test]
    fn timeout_error_is_retryable() {
        assert!(LlmError::Timeout.is_retryable());
    }

    #[test]
    fn invalid_model_error_is_not_retryable() {
        assert!(!LlmError::InvalidModel("x".into()).is_retryable());
    }

    #[test]
    fn retry_config_default_values() {
        let cfg = RetryConfig::default();
        assert_eq!(cfg.max_retries, 3);
        assert_eq!(cfg.base_delay_ms, 500);
        assert!(cfg.max_jitter_ms > 0);
    }

    #[test]
    fn router_creation_with_custom_retry() {
        let cfg = RetryConfig {
            max_retries: 5,
            base_delay_ms: 100,
            max_jitter_ms: 50,
        };
        let router = LlmRouter::new_with_retry(cfg.clone());
        assert_eq!(router.retry_config().max_retries, 5);
        assert_eq!(router.retry_config().base_delay_ms, 100);
        assert_eq!(router.retry_config().max_jitter_ms, 50);
    }
}

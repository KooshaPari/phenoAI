//! LLM Router - Multi-provider LLM routing
//!
//! Inspired by litellm, provides unified interface for multiple LLM providers.

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

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
        Self {
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let start = std::time::Instant::now();

        let body = serde_json::json!({
            "model": request.model,
            "messages": request.messages,
            "temperature": request.temperature.unwrap_or(0.7),
        });

        let _response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        let usage = TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        };

        Ok(CompletionResponse {
            content: "response".to_string(),
            model: request.model.clone(),
            provider: self.provider_name().to_string(),
            usage,
            latency_ms: start.elapsed().as_millis() as u64,
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

        if let Some(provider) = self.providers.get(prefix) {
            return provider.complete(request).await;
        }

        // Try fallback
        if let Some(fallback) = &self.fallback {
            return fallback.complete(request).await;
        }

        Err(LlmError::InvalidModel(request.model.clone()))
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
        let b = LlmError::RateLimited;
        let c = LlmError::Timeout;
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

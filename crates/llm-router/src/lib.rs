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

        let response = self.client
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

    pub fn set_fallback(&self, provider: Arc<dyn LlmProvider>) {
        self.fallback = Some(provider);
    }

    pub async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        // Route based on model prefix
        let (prefix, _) = request.model.split_once('/').unwrap_or((&request.model, ""));
        
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
    }
}

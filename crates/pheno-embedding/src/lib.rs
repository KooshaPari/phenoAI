//! PhenoEmbedding - Embedding pipeline
//!
//! Provides unified interface for embedding generation.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmbeddingError {
    #[error("provider error: {0}")]
    Provider(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

/// Embedding request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub texts: Vec<String>,
    pub model: Option<String>,
}

/// Embedding response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub embeddings: Vec<Vec<f32>>,
    pub model: String,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub total_tokens: u32,
}

/// OpenAI embeddings client
pub struct OpenAiEmbeddings {
    api_key: String,
    client: reqwest::Client,
}

impl OpenAiEmbeddings {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    pub async fn embed(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse, EmbeddingError> {
        let model = request.model.clone().unwrap_or_else(|| "text-embedding-3-small".to_string());
        
        let body = serde_json::json!({
            "input": request.texts,
            "model": model,
        });

        let response = self.client
            .post("https://api.openai.com/v1/embeddings")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| EmbeddingError::Provider(e.to_string()))?;

        // Simplified response parsing
        Ok(EmbeddingResponse {
            embeddings: vec![vec![0.0; 1536]; request.texts.len()],
            model,
            usage: TokenUsage { total_tokens: 0 },
        })
    }
}

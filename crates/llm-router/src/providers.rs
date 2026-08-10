//! Cheap-LLM provider presets and configurations.
//!
//! This module provides preset configurations for popular OpenAI-compatible
//! LLM providers that offer cost-effective inference (the "cheap-llm" tier).
//!
//! All providers use the OpenAI Chat Completions API shape with provider-specific:
//! - `base_url`: API endpoint
//! - `api_key`: Authentication credential (typically read from env)
//! - `name`: Identifier used in routing/logging
//!
//! To use a provider, construct an `OpenAiProvider` with the appropriate config:
//!
//! ```no_run
//! use phenoai_llm_router::{OpenAiProvider, LlmProvider, CompletionRequest};
//! use phenoai_llm_router::providers::minimax;
//!
//! let provider = OpenAiProvider::with_config(minimax());
//! // ... use provider.complete(&request)
//! ```

use serde::{Deserialize, Serialize};

/// Configuration for an OpenAI-compatible provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Stable identifier for the provider (e.g., "minimax", "kimi", "fireworks").
    pub name: String,
    /// Base URL for the provider's API (no trailing slash).
    pub base_url: String,
    /// Environment variable name that contains the API key.
    pub api_key_env: String,
    /// Default model to use when not specified in the request.
    pub default_model: String,
}

impl ProviderConfig {
    /// Read the API key from the environment. Returns `None` if unset.
    pub fn resolve_api_key(&self) -> Option<String> {
        std::env::var(&self.api_key_env).ok()
    }
}

/// Minimax provider (cost-optimized inference tier).
pub fn minimax() -> ProviderConfig {
    ProviderConfig {
        name: "minimax".into(),
        base_url: "https://api.minimax.chat/v1".into(),
        api_key_env: "MINIMAX_API_KEY".into(),
        default_model: "MiniMax-Text-01".into(),
    }
}

/// Moonshot Kimi provider (long-context, cost-effective).
pub fn kimi() -> ProviderConfig {
    ProviderConfig {
        name: "kimi".into(),
        base_url: "https://api.moonshot.cn/v1".into(),
        api_key_env: "MOONSHOT_API_KEY".into(),
        default_model: "moonshot-v1-8k".into(),
    }
}

/// Fireworks AI provider (fast open-model inference).
pub fn fireworks() -> ProviderConfig {
    ProviderConfig {
        name: "fireworks".into(),
        base_url: "https://api.fireworks.ai/inference/v1".into(),
        api_key_env: "FIREWORKS_API_KEY".into(),
        default_model: "accounts/fireworks/models/llama-v3p1-8b-instruct".into(),
    }
}

/// All cheap-llm presets for fleet-wide discovery.
pub fn presets() -> Vec<ProviderConfig> {
    vec![minimax(), kimi(), fireworks()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_have_unique_names() {
        let binding = presets();
        let mut names: Vec<&str> = binding.iter().map(|p| p.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 3, "expected 3 unique provider names");
    }

    #[test]
    fn minimax_resolves() {
        let p = minimax();
        assert_eq!(p.name, "minimax");
        assert!(p.base_url.starts_with("https://"));
        assert!(!p.api_key_env.is_empty());
    }

    #[test]
    fn kimi_resolves() {
        let p = kimi();
        assert_eq!(p.name, "kimi");
        assert!(p.base_url.contains("moonshot"));
    }

    #[test]
    fn fireworks_resolves() {
        let p = fireworks();
        assert_eq!(p.name, "fireworks");
        assert!(p.base_url.contains("fireworks"));
    }
}

// Integration tests for llm-router crate
// Traces to: FR-001

use llm_router::{
    CompletionRequest, CompletionResponse, LlmError, LlmRouter, LlmProvider, Message, TokenUsage,
};

/// Mock provider for testing
struct MockProvider {
    name: String,
    should_fail: bool,
}

impl MockProvider {
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

#[::async_trait::async_trait]
impl LlmProvider for MockProvider {
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, LlmError> {
        if self.should_fail {
            return Err(LlmError::Provider("Mock failure".to_string()));
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

#[test]
fn test_completion_request_serialization() {
    let request = CompletionRequest {
        model: "gpt-4".to_string(),
        messages: vec![
            Message {
                role: "user".to_string(),
                content: "Hello".to_string(),
            },
        ],
        temperature: Some(0.7),
        max_tokens: Some(100),
        timeout_ms: Some(30000),
    };

    let json = serde_json::to_string(&request).expect("Should serialize");
    assert!(json.contains("gpt-4"));
    assert!(json.contains("Hello"));

    let deserialized: CompletionRequest =
        serde_json::from_str(&json).expect("Should deserialize");
    assert_eq!(deserialized.model, "gpt-4");
}

#[test]
fn test_completion_response_serialization() {
    let response = CompletionResponse {
        content: "Test response".to_string(),
        model: "gpt-4".to_string(),
        provider: "openai".to_string(),
        usage: TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
        },
        latency_ms: 150,
    };

    let json = serde_json::to_string(&response).expect("Should serialize");
    assert!(json.contains("Test response"));

    let deserialized: CompletionResponse =
        serde_json::from_str(&json).expect("Should deserialize");
    assert_eq!(deserialized.content, "Test response");
    assert_eq!(deserialized.usage.total_tokens, 30);
}

#[test]
fn test_llm_router_creation() {
    let router = LlmRouter::new();
    assert!(router.providers.is_empty());
    assert!(router.fallback.is_none());
}

#[test]
fn test_llm_router_register_provider() {
    use std::sync::Arc;

    let router = LlmRouter::new();
    let provider = Arc::new(MockProvider::new("test-provider"));

    router.register_provider("test", provider);

    assert_eq!(router.providers.len(), 1);
    assert!(router.providers.contains_key("test"));
}

#[test]
fn test_llm_router_set_fallback() {
    use std::sync::Arc;

    let router = LlmRouter::new();
    let fallback = Arc::new(MockProvider::new("fallback"));

    router.set_fallback(fallback);

    assert!(router.fallback.is_some());
}

#[test]
fn test_llm_error_display() {
    let err = LlmError::Provider("test error".to_string());
    assert_eq!(format!("{}", err), "provider error: test error");

    let err = LlmError::RateLimited;
    assert_eq!(format!("{}", err), "rate limited");

    let err = LlmError::Timeout;
    assert_eq!(format!("{}", err), "timeout");

    let err = LlmError::InvalidModel("gpt-5".to_string());
    assert_eq!(format!("{}", err), "invalid model: gpt-5");
}

#[test]
fn test_message_creation() {
    let msg = Message {
        role: "assistant".to_string(),
        content: "I am here to help".to_string(),
    };

    assert_eq!(msg.role, "assistant");
    assert_eq!(msg.content, "I am here to help");
}

#[test]
fn test_token_usage() {
    let usage = TokenUsage {
        prompt_tokens: 100,
        completion_tokens: 200,
        total_tokens: 300,
    };

    assert_eq!(usage.prompt_tokens, 100);
    assert_eq!(usage.completion_tokens, 200);
    assert_eq!(usage.total_tokens, 300);
}

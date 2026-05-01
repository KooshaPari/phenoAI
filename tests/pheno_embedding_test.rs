// Integration tests for pheno-embedding crate
// Traces to: FR-001

use pheno_embedding::{EmbeddingRequest, EmbeddingResponse, TokenUsage};

#[test]
fn test_embedding_request_creation() {
    let request = EmbeddingRequest {
        texts: vec!["Hello world".to_string(), "Rust is great".to_string()],
        model: Some("text-embedding-3-small".to_string()),
    };

    assert_eq!(request.texts.len(), 2);
    assert_eq!(request.model, Some("text-embedding-3-small".to_string()));
}

#[test]
fn test_embedding_request_default_model() {
    let request = EmbeddingRequest {
        texts: vec!["Test text".to_string()],
        model: None,
    };

    assert!(request.model.is_none());
}

#[test]
fn test_embedding_response_creation() {
    let response = EmbeddingResponse {
        embeddings: vec![
            vec![0.1, 0.2, 0.3],
            vec![0.4, 0.5, 0.6],
        ],
        model: "text-embedding-3-small".to_string(),
        usage: TokenUsage { total_tokens: 100 },
    };

    assert_eq!(response.embeddings.len(), 2);
    assert_eq!(response.embeddings[0].len(), 3);
    assert_eq!(response.model, "text-embedding-3-small");
    assert_eq!(response.usage.total_tokens, 100);
}

#[test]
fn test_token_usage() {
    let usage = TokenUsage { total_tokens: 500 };

    assert_eq!(usage.total_tokens, 500);
}

#[test]
fn test_embedding_request_serialization() {
    let request = EmbeddingRequest {
        texts: vec!["First".to_string(), "Second".to_string()],
        model: Some("embed-model".to_string()),
    };

    let json = serde_json::to_string(&request).expect("Should serialize");
    assert!(json.contains("First"));
    assert!(json.contains("Second"));
    assert!(json.contains("embed-model"));

    let deserialized: EmbeddingRequest =
        serde_json::from_str(&json).expect("Should deserialize");
    assert_eq!(deserialized.texts.len(), 2);
}

#[test]
fn test_embedding_response_serialization() {
    let response = EmbeddingResponse {
        embeddings: vec![vec![0.1, 0.2, 0.3]],
        model: "test-model".to_string(),
        usage: TokenUsage { total_tokens: 50 },
    };

    let json = serde_json::to_string(&response).expect("Should serialize");
    assert!(json.contains("test-model"));
    assert!(json.contains("embeddings"));

    let deserialized: EmbeddingResponse =
        serde_json::from_str(&json).expect("Should deserialize");
    assert_eq!(deserialized.model, "test-model");
    assert_eq!(deserialized.usage.total_tokens, 50);
}

#[test]
fn test_single_text_embedding() {
    let request = EmbeddingRequest {
        texts: vec!["Single text input".to_string()],
        model: None,
    };

    assert_eq!(request.texts.len(), 1);
    assert_eq!(request.texts[0], "Single text input");
}

#[test]
fn test_large_batch_embedding_request() {
    let texts: Vec<String> = (0..100).map(|i| format!("Text number {}", i)).collect();

    let request = EmbeddingRequest {
        texts,
        model: Some("text-embedding-3-large".to_string()),
    };

    assert_eq!(request.texts.len(), 100);
    assert_eq!(request.texts[99], "Text number 99");
}

#[test]
fn test_empty_texts_embedding() {
    let request = EmbeddingRequest {
        texts: vec![],
        model: None,
    };

    assert!(request.texts.is_empty());
}

#[test]
fn test_embedding_vector_dimensions() {
    // Test various embedding dimension sizes
    let small_embedding = vec![0.1; 384];
    let medium_embedding = vec![0.2; 1536];
    let large_embedding = vec![0.3; 3072];

    assert_eq!(small_embedding.len(), 384);
    assert_eq!(medium_embedding.len(), 1536);
    assert_eq!(large_embedding.len(), 3072);

    let response = EmbeddingResponse {
        embeddings: vec![small_embedding, medium_embedding, large_embedding],
        model: "multi-model".to_string(),
        usage: TokenUsage { total_tokens: 1000 },
    };

    assert_eq!(response.embeddings.len(), 3);
    assert_eq!(response.embeddings[0].len(), 384);
    assert_eq!(response.embeddings[1].len(), 1536);
    assert_eq!(response.embeddings[2].len(), 3072);
}

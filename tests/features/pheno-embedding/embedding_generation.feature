@pheno-embedding
Feature: Embedding generation — OpenAI provider
  As a Phenotype service that needs vector embeddings
  I want a unified embedding interface with an OpenAI implementation
  So that I can plug in different providers without rewriting call-sites

  Background:
    Given an OpenAiEmbeddings client with model "text-embedding-3-small"

  @happy-path @mvp
  Scenario: Single text is embedded
    When I call embeddings.embed("Hello, world!")
    Then the response is an EmbeddingResponse
    And the response contains 1 vector
    And each vector has 1536 dimensions

  @batch
  Scenario: Batch of texts is embedded in one call
    When I call embeddings.embed(["Hello", "World", "Foo", "Bar"])
    Then the response contains 4 vectors
    And the order of vectors matches the order of input texts

  @error
  Scenario: Empty input returns an error
    When I call embeddings.embed([])
    Then an EmbeddingError::InvalidInput is returned

  @error
  Scenario: Provider returns 401 returns EmbeddingError::Provider
    Given the OpenAI client is configured to return HTTP 401
    When I call embeddings.embed("Hello")
    Then an EmbeddingError::Provider is returned
    And the error message includes "authentication"

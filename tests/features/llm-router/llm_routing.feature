@llm-router
Feature: LLM Router — multi-provider routing with fallback
  As a Phenotype service that needs completion calls
  I want to route requests across configured LLM providers with fallback semantics
  So that one provider being down doesn't break my service

  Background:
    Given a configured LlmRouter with two providers
      | name      | kind   | priority |
      | openai-a  | openai | 1        |
      | openai-b  | openai | 2        |

  @happy-path @mvp
  Scenario: Primary provider succeeds
    When I call router.route("Summarize this text") on provider "openai-a"
    Then the response is returned from "openai-a"
    And no fallback occurred

  @fallback
  Scenario: Primary provider fails, fallback to secondary
    Given provider "openai-a" is configured to return error "RateLimited"
    When I call router.route("Summarize this text")
    Then the response is returned from "openai-b"
    And the fallback path was exercised exactly once

  @error
  Scenario: All providers fail
    Given both providers are configured to return error "ServiceUnavailable"
    When I call router.route("Summarize this text")
    Then an LlmError::AllProvidersFailed error is returned
    And no retry loop exceeds the bounded retry cap

  @observability
  Scenario: Each routed call emits a tracing event
    When I call router.route("Hello")
    Then a tracing event at INFO level is emitted
    And the event includes fields: provider, latency_ms, prompt_tokens

@mcp-server
Feature: MCP Server — tool and resource registration
  As an MCP-compatible client (e.g. Claude Code, Cursor)
  I want to discover and call tools/resources exposed by a Phenotype service
  So that I can use Phenotype capabilities inside my IDE

  Background:
    Given an McpServer named "phenotype-mcp"
    And the server registers a tool "list_journeys"
    And the server registers a resource "journey://catalog"

  @happy-path @mvp
  Scenario: Client lists available tools
    When a client sends a tools/list request
    Then the response contains "list_journeys" with its JSON schema
    And the response contains "list_resources" with its JSON schema

  @happy-path @mvp
  Scenario: Client calls a registered tool
    When a client sends a tools/call for "list_journeys" with empty args
    Then the response is a success
    And the response payload is a JSON array of journey manifests

  @error
  Scenario: Calling an unknown tool returns McpError::ToolNotFound
    When a client sends a tools/call for "this_tool_does_not_exist"
    Then the response is an error
    And the error code is McpError::ToolNotFound

  @resource
  Scenario: Client reads a registered resource
    When a client sends a resources/read for "journey://catalog"
    Then the response contains the catalog content
    And the content MIME type is "application/json"

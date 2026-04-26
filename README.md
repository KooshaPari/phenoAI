# phenoAI

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Quality Gate](https://github.com/KooshaPari/phenoAI/actions/workflows/quality-gate.yml/badge.svg)](https://github.com/KooshaPari/phenoAI/actions/workflows/quality-gate.yml)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

AI integration workspace for the Phenotype ecosystem — LLM routing, MCP server plumbing, and embedding primitives that Phenotype agents and services compose into higher-level AI behaviors.

**Part of the [Phenotype org](https://github.com/KooshaPari) ecosystem.** Shares CI reusables and conventions with [phenoShared](https://github.com/KooshaPari/phenoShared). Follows org conventions: conventional commits, `<type>/<topic>` branching, Apache-2.0 + MIT dual license.

## What it does

phenoAI is the Rust workspace that consolidates the AI-facing building blocks every Phenotype service eventually needs: routing a prompt to the right model/provider, exposing functionality over MCP, and producing / consuming vector embeddings. Keeping these in one workspace prevents each agent repo from picking its own (often incompatible) LLM client stack.

Downstream consumers are agent runtimes, the Phenotype daemon, and the spec-driven workflows that invoke LLMs for summarization, extraction, codegen, and retrieval.

## Status

**Active — scaffolding phase.** Three core crates are in place; public APIs are stabilizing. See [CHANGELOG.md](./CHANGELOG.md) for recent work.

## Requirements

- Rust stable (edition 2021)
- Provider credentials at runtime for whichever LLMs you route to (OpenAI, Anthropic, local, etc.)

## Quick start

```bash
# Build
cargo build --workspace

# Test
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --all -- --check
```

Use an individual crate as a git dependency:

```toml
[dependencies]
llm-router        = { git = "https://github.com/KooshaPari/phenoAI" }
mcp-server        = { git = "https://github.com/KooshaPari/phenoAI" }
pheno-embedding   = { git = "https://github.com/KooshaPari/phenoAI" }
```

## Structure

```
crates/
  llm-router/       # Multi-provider LLM routing with fallback, retry, and cost policy
  mcp-server/       # MCP server scaffolding for exposing Phenotype capabilities
  pheno-embedding/  # Embedding generation and vector primitives
```

## Design principles

- **Route by policy, not by hard-code.** `llm-router` makes provider selection declarative; cheap/fast/high-quality tiers are config, not code.
- **MCP first.** Capability surfaces are exposed over MCP so any MCP-aware client (IDEs, agents, daemons) can consume them uniformly.
- **Wrap, do not hand-roll.** Uses the official Anthropic, OpenAI, and `modelcontextprotocol` SDKs where available; adds Phenotype-specific policy on top.
- **Fail loudly on missing credentials.** Required providers that lack credentials hard-fail — no silent degradation to an in-memory stub in production.

## Crates Overview

### llm-router

Multi-provider LLM routing with cost policies and fallback support.

**Features:**
- Route requests to OpenAI, Anthropic, Google (Gemini), local models
- Cost-aware tier selection (cheap/standard/high-quality)
- Automatic retry with exponential backoff
- Provider fallback on failure
- Token counting and cost estimation
- Rate limiting and quota management

**Example:**
```rust
use llm_router::Router;

let router = Router::new()
    .add_provider("openai", openai_config)
    .add_provider("claude", anthropic_config)
    .with_cost_policy(CostPolicy::cheap());

let response = router.complete("Hello, world!").await?;
```

### mcp-server

MCP server scaffolding for exposing Phenotype capabilities to any MCP client.

**Features:**
- Server initialization and lifecycle management
- Tool registration with automatic schema generation
- Resource serving for text/binary content
- Prompt templates with composition
- Error handling and logging
- Testing utilities

**Example:**
```rust
use mcp_server::Server;

let server = Server::new("my-mcp-server")
    .version("1.0.0");

server.add_tool("read_file", "Read file contents", |path: String| {
    Box::pin(async move {
        Ok(fs::read_to_string(path).await?)
    })
});

server.run().await?;
```

### pheno-embedding

Embedding generation and vector storage primitives.

**Features:**
- Multi-model embedding support (OpenAI, Anthropic, local)
- Vector similarity search
- Batch processing and caching
- Serialization for persistence
- Async interface with async batch operations

**Example:**
```rust
use pheno_embedding::EmbeddingClient;

let client = EmbeddingClient::new("openai");
let embeddings = client.embed_batch(texts).await?;

// Compute similarity
let similarity = pheno_embedding::cosine_similarity(&emb1, &emb2);
```

## Integration Examples

### With heliosCLI

Use phenoAI crates to route agent code generation to optimal models:

```rust
// In heliosCLI's model selection logic
let router = llm_router::Router::new()
    .add_provider("cheap", gemini_config)      // Fast analysis
    .add_provider("standard", claude_config)    // Code tasks
    .add_provider("premium", gpt4_config);      // Complex logic

// Route by task type and context
let model = router.select_by_policy(task_type, token_budget)?;
```

### With AgilePlus

Use MCP server to expose AgilePlus tasks as tools to Claude and other agents:

```rust
// In AgilePlus MCP integration
server.add_tool("get_spec", "Get specification for feature", |spec_id: String| {
    Box::pin(async move {
        agileplus::get_spec(&spec_id).await
    })
});
```

### With phenotype-shared

Extends error handling and config from `phenotype-shared`:

```rust
use phenotype_shared::error::{PhenoError, Result};
use phenotype_shared::config::ConfigEntry;

fn route_request(config: &ConfigEntry) -> Result<Response> {
    // Leverage shared error types and config traits
    Ok(router.route_by_config(config).await?)
}
```

## Design Patterns

### Cost-Aware Routing

Models are selected based on input size and quality requirements:

```rust
let policy = CostPolicy::tiered()
    .cheap(|tokens| tokens < 1000)        // Use Gemini
    .standard(|tokens| tokens < 10000)    // Use Claude
    .premium(|tokens| tokens >= 10000);   // Use GPT-4
```

### Fallback Chains

If a provider fails, automatically try the next:

```rust
let router = Router::new()
    .add_provider("primary", primary_config)
    .add_provider("fallback1", fallback1_config)
    .add_provider("fallback2", fallback2_config)
    .with_fallback_policy(FallbackPolicy::Sequential);
```

### MCP Tool Composition

Combine multiple tools into higher-level workflows:

```rust
server.add_tool("summarize_and_extract", "Summarize and extract", |text: String| {
    Box::pin(async move {
        let summary = summarize(&text).await?;
        let extracted = extract_entities(&summary).await?;
        Ok((summary, extracted))
    })
});
```

## Testing

```bash
# Unit tests
cargo test --lib

# Integration tests (requires provider credentials)
cargo test --test '*' -- --ignored --test-threads 1

# Specific crate tests
cargo test -p llm-router
cargo test -p mcp-server
cargo test -p pheno-embedding

# With coverage
cargo tarpaulin --workspace --out Html
```

## Performance Characteristics

| Operation | Latency | Notes |
|-----------|---------|-------|
| Route selection | <1ms | Policy-based selection, no API call |
| LLM completion | 100-5000ms | Depends on provider and model size |
| Embedding | 50-500ms | Batch processing preferred |
| Vector search | <10ms | In-memory similarity search |

See [PERFORMANCE.md](./docs/PERFORMANCE.md) for benchmarks.

## Governance

- **Status**: Active — scaffolding phase
- **Language**: Rust (edition 2021)
- **Type**: Workspace of core AI infrastructure crates
- **Part of**: Phenotype Ecosystem
- **Testing**: All tests reference functional requirements (FR traceability)
- **Quality**: Full clippy pass with all targets required

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md). Ownership lives in [CODEOWNERS](./CODEOWNERS). Report security issues per [SECURITY.md](./SECURITY.md).

## References

- **Workspace Crates**: See individual crate READMEs for detailed API docs
- **Related Projects**: Uses phenotype-shared for error handling and config
- **Integrations**: Consumed by heliosCLI, AgilePlus, and Phenotype daemon
- **Architecture**: [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md)

## License

Dual-licensed under Apache-2.0 OR MIT. See [LICENSE-APACHE](./LICENSE-APACHE) and [LICENSE-MIT](./LICENSE-MIT).

## License

MIT — see [LICENSE](./LICENSE).

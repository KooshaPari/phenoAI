# phenoAI

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

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md). Ownership lives in [CODEOWNERS](./CODEOWNERS). Report security issues per [SECURITY.md](./SECURITY.md).

## License

Dual-licensed under Apache-2.0 OR MIT. See [LICENSE-APACHE](./LICENSE-APACHE) and [LICENSE-MIT](./LICENSE-MIT).

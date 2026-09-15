# phenotype-ops-mcp — MCP Server for NanoVM Instance Management

[![AI Slop Inside](https://sladge.net/badge.svg)](https://sladge.net)

Model Context Protocol (MCP) server for the nanoVMs `ops` unikernel toolchain, extended with Phenotype-specific features (auth, multi-tenant isolation, observability). Enables Claude agents and human developers to interact with nanoVM instances and images through a standardized MCP interface.

## Overview

**phenotype-ops-mcp** is a maintained Phenotype fork of the upstream nanoVMs `ops-mcp` server. It bridges the gap between agent tools and the nanoVMs runtime, providing MCP-compatible operations for instance lifecycle management, image management, and unikernel orchestration.

**Core Mission**: Provide a unified MCP interface for nanoVM operations within Phenotype's agent-driven infrastructure stack, enabling safe multi-tenant instance isolation and observable unikernel deployment.

## Technology Stack

- **Language**: Go (1.20+)
- **MCP Protocol**: Model Context Protocol (SSE transport)
- **Upstream**: nanoVMs `ops` toolchain (tracked via `upstream` remote)
- **Deployment**: Docker, systemd, or standalone binary
- **Testing**: Go testing framework with mocking

## Key Features

- **Instance Lifecycle Management**: Create, start, stop, delete nanoVM instances
- **Image Management**: List, build, pull nanoVM images from registries
- **MCP Integration**: Full MCP server implementation for Claude/Cursor integration
- **Multi-Tenant Isolation**: Namespace-aware instance grouping with access controls
- **Observability Hooks**: Structured logging and span instrumentation for distributed tracing
- **Authentication Support**: API key/token-based auth for remote deployments
- **Hot Reload**: Configuration reloading without instance downtime
- **CLI & API**: Both CLI tools and programmatic API for instance operations

## Quick Start

```bash
# Navigate to repo
cd /Users/kooshapari/CodeProjects/Phenotype/repos/phenotype-ops-mcp

# Build the MCP server
go build -o ops-mcp ./cmd/server

# Configure for Claude Desktop (macOS)
# Edit: ~/Library/Application Support/Claude/claude_desktop_config.json
cat > claude_desktop_config.json << 'EOF'
{
  "mcpServers": {
    "ops-mcp": {
      "command": "/path/to/ops-mcp",
      "env": {
        "PATH": "/bin:/usr/local/bin:/Users/<username>/.ops/bin",
        "HOME": "/Users/<username>",
        "SHELL": "/bin/zsh"
      }
    }
  }
}
EOF

# Test availability
go test ./...

# List instances via CLI
./ops-mcp instances list

# Create instance
./ops-mcp instances create my-instance redis-server
```

## Project Structure

```
phenotype-ops-mcp/
├── cmd/
│   └── server/
│       └── main.go             # MCP server entry point
├── internal/
│   ├── mcp/
│   │   ├── server.go           # MCP protocol implementation
│   │   └── tools.go            # MCP tool definitions
│   ├── ops/
│   │   ├── instance.rs         # Instance lifecycle manager
│   │   ├── image.go            # Image registry integration
│   │   └── isolation.go        # Multi-tenant namespace handling
│   ├── auth/
│   │   └── auth.go             # API key/token validation
│   └── observability/
│       └── tracing.go          # OpenTelemetry instrumentation
├── pkg/
│   ├── api/
│   │   └── client.go           # Programmatic API client
│   └── config/
│       └── config.go           # Configuration loader
├── examples/
│   ├── claude_desktop_config.json  # Claude Desktop config template
│   └── basic_instance_ops.go       # Example usage
├── tests/
│   └── integration_test.go     # End-to-end test suite
├── CLAUDE.md                   # Development guidelines
├── go.mod                      # Dependency manifest
├── go.sum                      # Dependency lock
└── README.md                   # This file
```

## MCP Tools Available

| Tool | Description | Example |
|------|-------------|---------|
| **instances:list** | List all nanoVM instances | List current instances |
| **instances:create** | Create new instance from image | Create redis-server instance |
| **instances:start** | Start stopped instance | Start my-instance |
| **instances:stop** | Gracefully stop running instance | Stop my-instance |
| **instances:delete** | Remove instance and cleanup | Delete my-instance |
| **instances:inspect** | Get instance details/status | Get my-instance status |
| **images:list** | List available nanoVM images | List images from registry |
| **images:build** | Build custom image from Dockerfile | Build my-app.img |
| **images:pull** | Pull image from remote registry | Pull ubuntu-22.04 |

## Upstream Synchronization

This is a **fork-extension pattern**: Phenotype customizations live here, upstream changes come via `upstream` remote.

```bash
# Sync with upstream (quarterly)
git fetch upstream main
git merge upstream/main

# Contribute generic improvements back
git push origin feature-branch
# Then open PR on nanovms/ops-mcp
```

See `UPSTREAM_SYNC.md` for sync schedule and procedure.

## Related Phenotype Projects

- **phenotype-nanovms-client** — Client library for ops-mcp server
- **phenotype-infrakit** — Infrastructure orchestration; consumes ops-mcp tools
- **Tracera** — Observability platform; consumes ops-mcp span data
- **AgilePlus** — Work tracking with ops integration points

## Configuration

Create `~/.phenotype-ops/config.toml`:

```toml
[server]
bind = "127.0.0.1:9090"
log_level = "info"

[auth]
enabled = true
api_keys = ["sk-ops-xxxxxx"]

[isolation]
default_namespace = "default"
enforce_multi_tenant = true
```

## License & Governance

Note: Very open to suggestions on how this all should work as this initial cut was done not having
ever used Claude or MCP.

## Tool Manifest

A canonical machine-readable manifest of every tool this server exposes lives at
[`tools.json`](./tools.json). It lists each tool's `name`, `description`, and
`inputSchema` (JSON Schema reflected from the handler argument struct) so
downstream consumers can discover the surface without reading `main.go`.

Regenerate after any tool change:

```
go run . --dump-tools
```

The [`Manifest check`](./.github/workflows/manifest-check.yml) workflow runs the
same command in CI and fails the build if `tools.json` drifts from the code, so
the manifest cannot go stale.

Note: `metoro-io/mcp-golang@v0.13.0` does not expose a public accessor for
registered tools (the internal `tools` map is unexported). The manifest is
built from the single `toolRegistrations()` source of truth in `main.go` and
re-reflects input schemas via `invopop/jsonschema` (the same library the SDK
uses internally). If upstream adds a `ListTools()` accessor, this plumbing can
be simplified.

Licensed under Apache-2.0 (preserved from upstream). See `LICENSE`. Governance,
development guidelines, and agent contract in `CLAUDE.md` and `AGENTS.md`.
Functional requirements and FR-to-test mapping live in
`FUNCTIONAL_REQUIREMENTS.md` when present.

# Contributing to MCPForge

MCPForge is the Phenotype fork of [`isaacphi/mcp-language-server`](https://github.com/isaacphi/mcp-language-server) — an MCP server that exposes language-server semantic tools (definitions, references, rename, diagnostics) to LLM clients.

## Prerequisites

- Go (see `go.mod` for the pinned version)
- A target language server installed locally (`gopls`, `rust-analyzer`, `pyright`, `typescript-language-server`, etc.)
- `just` (for the bundled `justfile` recipes) — optional

## Getting Started

```bash
git clone https://github.com/KooshaPari/MCPForge.git
cd MCPForge
go mod download
go build ./...
```

## Development Workflow

1. **Branch from `main`**: `git checkout -b feat/<topic>`.
2. **Run tests**:
   ```bash
   go test ./...
   go test ./integrationtests/...
   ```
3. **Lint and format**:
   ```bash
   gofmt -l .
   go vet ./...
   ```
4. **Manual smoke test**: Run the server against a real workspace (see `README.md` "Setup" section) before opening a PR.
5. **Commit style**: Conventional commits (`feat:`, `fix:`, `docs:`).
6. **Open a PR**: Describe the change, list affected language servers, and include integration test coverage.

## Upstream Sync

This is a fork. When picking up upstream changes:
- Track `isaacphi/mcp-language-server` `main` as a remote.
- Rebase Phenotype-specific patches on top; never force-push without coordination.
- Document deviations from upstream in `ATTRIBUTION` or commit messages.

## Code Standards

- `go vet` clean.
- `gofmt` formatted.
- New LSP integrations include integration tests under `integrationtests/`.
- Respect the existing `internal/` package boundary.

## Reporting Issues

Open a GitHub issue with: target language server name + version, OS, Go version, MCP client (Claude Desktop, etc.), and reproduction steps.

## License

By contributing you agree your work is licensed under the project `LICENSE`.

> **Pinned references (Phenotype-org)**
> - MSRV: see rust-toolchain.toml
> - cargo-deny config: see deny.toml
> - cargo-audit: rustsec/audit-check@v2 weekly
> - Branch protection: 1 reviewer required, no force-push
> - Authority: phenotype-org-governance/SUPERSEDED.md

> **REDIRECT (2026-06-17)** — Per [ADR-017](https://github.com/KooshaPari/PhenoSpecs/blob/main/adrs/017-mcp-polyrepo-boundaries.md) and [ADR-019](https://github.com/KooshaPari/PhenoSpecs/blob/main/adrs/019-mcp-runtime-implementation-deps.md).
> **Deployable MCP servers** → [PhenoMCPServers](https://github.com/KooshaPari/PhenoMCPServers) (`servers/` catalog).
> **Fleet runtime** (dispatch, argv, cheap-llm routing) → [substrate](https://github.com/KooshaPari/substrate).
> This repo is a **superseded migration source** — see [`docs/boundary/DISPOSITION.md`](docs/boundary/DISPOSITION.md). Do not add new server entrypoints here.

# PhenoMCP

[![Build](https://img.shields.io/github/actions/workflow/status/KooshaPari/PhenoMCP/ci.yml?branch=main&label=build)](https://github.com/KooshaPari/PhenoMCP/actions)
[![Release](https://img.shields.io/github/v/release/KooshaPari/PhenoMCP?include_prereleases&sort=semver)](https://github.com/KooshaPari/PhenoMCP/releases)
[![License](https://img.shields.io/github/license/KooshaPari/PhenoMCP)](LICENSE)
[![Phenotype](https://img.shields.io/badge/Phenotype-org-blueviolet)](https://github.com/KooshaPari)

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://opensource.org/licenses/MIT)
[![CI](https://github.com/KooshaPari/PhenoMCP/actions/workflows/ci.yml/badge.svg)](https://github.com/KooshaPari/PhenoMCP/actions/workflows/ci.yml)
[![Language: Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![AI Slop Inside](https://sladge.net/badge.svg)](https://sladge.net)

> **Status: Experimental / Pre-foundational — Active Development**
>
> This repository is an early scaffold. The top-level `pheno-mcp` binary
> (`src/main.rs`) is a scaffold. Real work is happening inside the
> `crates/` and `bindings/` directories described below.

PhenoMCP is the planned home for a Model Context Protocol server plus a set
of backend adapter crates and cross-language bindings used by the Phenotype
ecosystem. It is **not** a usable MCP server yet — there is no tool surface,
no resources layer, no auth, and no transport implementation in the root
binary.

## Quick Start

### 1. Install

```bash
# Clone the repository
git clone https://github.com/KooshaPari/PhenoMCP.git
cd PhenoMCP

# Ensure Rust toolchain is installed (see rust-toolchain.toml for MSRV)
rustup show
```

### 2. Run the scaffold binary

```bash
# Build and run the experimental Rust root binary (scaffold only)
cargo run
```

The root binary currently prints PhenoMCP and exits. It is **not** yet a functional MCP server.

### 3. Run the canonical Python MCP server (supported entrypoint)

```bash
# Install Python dependencies and run the supported FastMCP bridge
pip install -e .
python -m pheno_mcp   # stdio MCP server
```

For the full org MCP registry and tool naming conventions, see docs/MCP-CATALOG.md.

## What actually exists today

### Workspace layout

```
.
├── Cargo.toml              # workspace root, members = ["crates/*"]
├── src/main.rs             # scaffold: prints "PhenoMCP"
├── crates/
│   ├── pheno-meilisearch/      # Meilisearch HTTP client (reqwest, scaffold)
│   ├── pheno-qdrant/           # Qdrant HTTP client (reqwest, scaffold)
│   └── phenotype-surrealdb/    # SurrealDB wrapper crate (surrealdb 3.0)
├── bindings/
│   ├── swift/Sources/PhenoMCP/   # Swift binding scaffold
│   ├── kotlin/src/               # Kotlin binding scaffold
│   └── csharp/src/               # C# binding scaffold
├── docs/                   # design notes
├── integration-tests/      # scaffold
├── go.mod                  # scaffold
├── package.json            # scaffold
├── pyproject.toml          # scaffold
└── ADR.md, CHARTER.md, PLAN.md, PRD.md, VERSION
```

### Backend adapter crates (`crates/`)

These are early-stage and exist primarily as workspace members with
dependencies wired up. None should be considered production-ready.

| Crate                  | Purpose                                                | Key deps                       |
|------------------------|--------------------------------------------------------|--------------------------------|
| `pheno-meilisearch`    | HTTP client for Meilisearch                            | `reqwest`, `tokio`, `thiserror`|
| `pheno-qdrant`         | HTTP client for Qdrant vector DB                       | `reqwest`, `tokio`, `thiserror`|
| `phenotype-surrealdb`  | Thin wrapper over upstream `surrealdb` 3.0 with the WS protocol feature | `surrealdb` 3.0, `tokio` |

Each crate currently has a single `src/lib.rs`. Treat APIs as unstable.

### Language bindings (`bindings/`)

Scaffolds only. There is no working FFI/UniFFI layer yet — these directories
exist so the Swift / Kotlin / C# packaging story can be developed in
parallel with the Rust core.

- `bindings/swift/Sources/PhenoMCP/`
- `bindings/kotlin/src/`
- `bindings/csharp/src/`

If you are looking for a working PhenoMCP MCP server today, use the **canonical
Python FastMCP bridge** (see below) — the Rust root binary is experimental.

## Canonical MCP entrypoint

The supported PhenoMCP MCP server is the **Python FastMCP bridge**:

```bash
python -m pheno_mcp   # stdio MCP server (FastMCP), streamable-HTTP planned
```

It exposes the org governance/agent/knowledge/policy/session/workflow tools via
FastMCP (the Phenotype framework convention). All NEW org MCP tools land here.
The Rust root binary is **experimental/future** and is not yet a functional MCP
server (see "What does not exist yet"). See `docs/MCP-CATALOG.md` for the full
org MCP registry and the `<server>_<verb>_<noun>` tool-naming standard.

## What does **not** exist yet (Rust root binary)

The previous version of this README claimed a Quick Start, a `src/tools/`
module, a `src/resources/` module, an `src/auth/` module, and an
`src/adapters/` module. **None of those exist.** They were aspirational and
have been removed from this README to avoid misleading readers.

There is currently:

- No MCP transport (stdio/HTTP/WebSocket) implementation in the root binary.
- No tool registry, resource provider, or prompt surface.
- No auth layer.
- No published packages on crates.io, npm, PyPI, or any binding registry.
- No CI gates verifying behavior beyond `cargo build`.

## Building

The workspace compiles with a recent stable Rust toolchain (edition 2024,
resolver 3 — Rust 1.85+ required):

```bash
cargo build --workspace
```

The `pheno-mcp` binary will compile and, when run, print the program name
and exit. That is its entire current behavior.

## Roadmap

See `PLAN.md`, `PRD.md`, `ADR.md`, and `CHARTER.md` in this repository for
the intended direction. Those documents describe the target architecture;
this README describes the current state.

## Docs surface

The `docs/` directory now serves as the docs landing surface for this scaffold.
It exposes:

- the current project overview
- research notes for MCP and multi-language AI SDKs
- the feature coverage matrix
- the architecture notes behind `ADR-001` through `ADR-005`
- the STRIDE threat model at [`docs/security/threat-model.md`](docs/security/threat-model.md)
- the security policy at [`SECURITY.md`](SECURITY.md)

## License

Dual-licensed under MIT or Apache-2.0, at your option. See `LICENSE-MIT`
and `LICENSE-APACHE`.


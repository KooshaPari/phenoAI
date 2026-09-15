# Language tiers and domain roles (Phenotype fleet)

Last updated: 2026-06-17 (boundary audit, issue #7: phenotype-go-sdk retired)

This document is the **normative boundary** for where code may live by language
and by **domain role**. Language-based umbrella repos (e.g. a generic
`phenotype-rust-sdk`) are **anti-patterns** unless they own a single narrow role.

Work **in parallel** across tier-0 forks and domain repos; do not serialize on
one language.

---

## Language tiers

```
┌─────────────────────────────────────────────────────────────┐
│ Tier 0 — CORE (max perf, protocol truth, codegen source)    │
│   Rust · Zig · Mojo                                         │
│   PhenoFastMCP-rust (fastmcp_rust) · PhenoRMCP (rmcp) · substrate · phenoUtils │
└───────────────────────────┬─────────────────────────────────┘
                            │ FFI / gRPC / schema / wasm
┌───────────────────────────▼─────────────────────────────────┐
│ Tier 1 — EDGE GATEWAYS (justified Go; not bulk app logic)   │
│   Go — microservices, ops glue, LSP bridges, HTTP MCP       │
│   PhenoFastMCP-go (active) · MCPForge, phenotype-ops-mcp retired │
└───────────────────────────┬─────────────────────────────────┘
                            │ thin clients / admin / IDE / scripts
┌───────────────────────────▼─────────────────────────────────┐
│ Tier 2 — BINDINGS & APPS (only where core rebuild ≠ worth it)│
│   Python 3.14+ (uv) · Bun · TS 7 preview · C# · Java · …    │
│   PhenoFastMCP (fastmcp fork) · PhenoMCPServers · agents    │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────┐
│ Tier 3 — SCRIPTING                                          │
│   Shell · PowerShell — install, glue, CI one-liners         │
└─────────────────────────────────────────────────────────────┘
```

### Go placement rule

Go is **not** a second core. Use Go only with **written justification**:

| Allowed | Example |
|---------|---------|
| HTTP/SSE MCP edge server | PhenoFastMCP-go (active); MCPForge, ops-mcp deprecated |
| Microservice boundary | gateway in front of Rust substrate |
| Upstream SDK is Go-native | mark3labs/mcp-go fork |

| Not allowed | Move to tier 0/2 |
|-------------|-------------------|
| Business logic duplicated from Rust core | substrate planner, claims |
| “Because we had a Go SDK repo” | phenotype-go-sdk catch-all |

### Python placement rule

- **PhenoFastMCP** = fastmcp fork (framework + rapid server prototyping).
- **PhenoMCPServers** = deployable servers (may be Python today; Rust rewrite when hot).
- **phenotype-python-sdk** = optional **dependency umbrella** for Python apps (`uv` workspace), **not** protocol core.
- Prefer **Python 3.14+** and **uv** for new Python packages.

### TypeScript / Bun

- **TS 7 preview** / **Bun** for IDE plugins, web admin, client wiring — tier 2 edges.
- Do not host protocol core in TS when Rust/Zig exists.

---

## Domain roles (not language buckets)

| Role | Owner repo(s) | Tier | Notes |
|------|---------------|------|-------|
| **MCP framework** | PhenoFastMCP (py), PhenoFastMCP-go, PhenoFastMCP-rust | 0–2 per lang | Fork upstream; superset merges in parallel |
| **MCP implementations** | PhenoMCPServers | 2 (migrate hot paths to 0) | servers, skills, plugins, agents |
| **Fleet runtime** | substrate | 0 Rust | dispatch, argv, cheap-llm CLI, driver-http |
| **Rust utilities / ports** | phenoUtils, phenoShared | 0 | hex ports, crypto, fs — not “rust-sdk” |
| **Project bootstrap** | HexaKit | meta | templates point at roles above |
| **Python dep extras** | phenotype-py-extras → phenotype-python-sdk | 2 | lazy `[mcp]` groups only |
| **Graphics** | phenotype-gfx | 0 Rust + C# edge | model: core in Rust, Unity/interop in C# |

### Retire / avoid

| Pattern | Why | Replacement |
|---------|-----|-------------|
| **phenotype-rust-sdk** (generic) | Language bucket, hides domains | phenoUtils + PhenoFastMCP-rust + substrate crates |
| **phenotype-go-sdk** (catch-all) | Same for Go | PhenoFastMCP-go + justified edge services only |
| **McpKit** as lib warehouse | Duplicates framework forks | PhenoFastMCP* forks + registry |
| Large logic in Python MCP servers | Wrong tier for hot paths | Implement in Rust; thin Python shim until cutover |

---

## Parallel workstreams (MCP framework)

These proceed **simultaneously**; sync weekly on schema/registry only:

| Stream | Repo | Upstream |
|--------|------|----------|
| Py framework | PhenoFastMCP | PrefectHQ/fastmcp |
| Go framework | PhenoFastMCP-go | mark3labs/mcp-go |
| Rust framework | PhenoFastMCP-rust | Dicklesworthstone/fastmcp_rust |
| Rust spec SDK | PhenoRMCP | modelcontextprotocol/rust-sdk |
| Implementations | PhenoMCPServers | catalog/registry.yaml |
| Runtime | substrate | driver-* crates |

Tier-0 **Zig/Mojo** MCP cores are future forks/issues — same fork-parent rules as Rust.

**Trigger to start:** once the **rmcp superset** in `PhenoFastMCP-rust` stabilizes
(API surface + ergonomics layer), evaluate Zig and Mojo in **parallel** as
sibling forks. Each becomes its own gh repo:

| Stream | Future repo | Upstream (planned) | Status |
|--------|-------------|--------------------|--------|
| Zig framework | `PhenoFastMCP-zig` | `modelcontextprotocol/rust-sdk` (rmcp) | future |
| Mojo framework | `PhenoFastMCP-mojo` | `modelcontextprotocol/rust-sdk` (rmcp) | future |

**Anti-pattern guard:** these lanes are **not** added to `phenotype-rust-sdk`.
They are first-class forks with their own GH repos, mirroring the
`PhenoFastMCP-rust` fork-parent rule. The `phenotype-rust-sdk` bucket is the
retired language umbrella; do not reintroduce it via Zig/Mojo.

---

## Decision checklist (new code)

1. **Which domain role?** (framework / implementation / runtime / bootstrap)
2. **Which tier?** Default tier 0 unless edge justification doc exists.
3. **Existing owner?** Extend that repo; do not create `phenotype-<lang>-sdk`.
4. **Parallel?** If another tier-0 lane can proceed independently, start it.

---

## Cross-links

- `catalog/registry.yaml` — framework + server entries
- PhenoFastMCP `PHENO.md` — Python fork policy
- PhenoFastMCP-go / PhenoFastMCP-rust `PHENO.md` — sibling forks

---

## Boundary audit — issue #7 (phenotype-go-sdk retired)

`phenotype-go-sdk` was a catch-all Go repo that consolidated `PlatformKit`,
`DevHex`, and `McpKit`. It violated the tier model in two ways:

1. **Language bucket, not domain role.** It owned no single narrow role and
   shadowed the per-domain forks (PhenoFastMCP-go, MCPForge, and phenotype-ops-mcp, all now retired/deprecated).
2. **Hidden tier violations.** Cross-domain Go code drifted into business
   logic that belongs in the tier-0 Rust core (substrate planner, claims,
   dispatch).

**Decision (2026-06-17):** `phenotype-go-sdk` is **retired / archived**.
Its packages are split by domain role:

| Old umbrella package | Domain role | New owner repo | Tier |
|----------------------|-------------|----------------|------|
| MCP framework bits   | mcp-framework | PhenoFastMCP-go | 1 |
| LSP / HTTP MCP server | retired edge (absorbed) | MCPForge | 1 |
| NanoVM / ops bindings | retired edge (pending consolidation) | phenotype-ops-mcp | 1 |
| Generic helpers (`PlatformKit`, `DevHex`) | folded into per-domain forks above | — | — |
| Cross-domain lib dumps (`McpKit`) | **deleted** — duplicated PhenoFastMCP-go | — | — |

### Surviving tier-1 Go inventory (written justification)

Active Go repos in the fleet must appear here with a justification that maps
to one of the allowed placement rules in the table above. Deprecated or archived
repos may remain in `servers` with `status: deprecated`.

| Repo | Role | Justification (why Go, not tier-0) |
|------|------|------------------------------------|
| `KooshaPari/PhenoFastMCP-go` | mcp-framework | Upstream `mark3labs/mcp-go` is Go-native; fork is required for HTTP/SSE MCP edge servers. |
| `KooshaPari/substrate` (Go client only) | edge-gateway | Thin HTTP client for the tier-0 substrate API. Reuses `PhenoFastMCP-go` transport; no domain logic. |

### What does NOT live in Go

- Substrate planner, claims, dispatch, cheap-llm routing → **tier-0 Rust** (substrate).
- Hex ports, crypto, fs utilities → **tier-0 Rust** (phenoUtils, phenoShared).
- Protocol core / schema generators → **tier-0** (Rust/Zig/Mojo).
- MCP server *implementations* here in `PhenoMCPServers` → **tier-2 Python** (fastmcp fork), migrating hot paths to tier-0.

### Boundary check (this repo)

`PhenoMCPServers` ships **no Go code**. Active Go pointers are listed under
`go_tier1_inventory`; deprecated Go repos are documented in `docs/retire/RETIRED-MCP-REPOS.md`.
Adding a new Go package to this repo is a tier violation; open an issue
referencing this section instead.

# PhenoMCP — Functional & Non-Functional Requirements

**Scope:** Backfilled catalog for Tracera + AgilePlus ingestion.
**Schema version:** 1.
**Test baseline:** 260 Python tests (+ 2 skipped on Python 3.14a) + 19 Rust tests = 279 total passing (2026-05-29).
**ID namespace:** FR-MCP-NNN / NFR-MCP-NNN.

---

## Functional Requirements

### FR-MCP-001 — pheno_mcp Python Package

**Title:** `pheno_mcp` installable Python package exposing Server, Tool, Resource, Prompt, Client.

**Description:** The `pheno_mcp` package (under `python/src/pheno_mcp/`) is the primary
FastMCP-convention Python surface for PhenoMCP. It provides `Server`, `ServerConfig`,
`Tool`, `Resource`, `Prompt`, and `Client` as first-class public symbols re-exported from
`python/src/pheno_mcp/__init__.py`. The package is managed by `uv`/`pyproject.toml` with
`py.typed` marker for static-analysis compatibility.

**Acceptance criteria:**
- `from pheno_mcp import Server, Tool, Resource, Prompt, Client` succeeds after `uv sync`.
- `pyproject.toml` declares the package and its entry points; `uv.lock` is committed.
- `py.typed` is present and `mypy` resolves types without `--ignore-missing-imports`.
- CI (`ubuntu-24.04`, SHA-pinned) runs `uv run pytest` and all tests pass.

**Traceability:**
- PR #76 (pheno_mcp Python package + workflow hygiene ubuntu-24.04 + SHA pins)
- Tests: `python/tests/test_init.py`

---

### FR-MCP-002 — MCP Server Registration (Tools / Resources / Prompts)

**Title:** `Server` registers and lists Tools, Resources, and Prompts by unique key.

**Description:** `Server` maintains three in-memory registries keyed by `tool.name`,
`resource.uri`, and `prompt.name`. Registration is idempotent — re-registering the same
key overwrites the existing entry. `list_tools()`, `list_resources()`, `list_prompts()`
return serialised dicts suitable for a JSON-RPC `tools/list` response. Each object's
`to_dict()` maps to the MCP wire format (`inputSchema`, `mimeType`, etc.).

**Acceptance criteria:**
- `register_tool(t)` followed by `list_tools()` returns exactly one entry with matching name.
- Re-registering the same key replaces the prior entry (no duplicates).
- `list_resources()` / `list_prompts()` follow the same contract.
- All three `to_dict()` methods serialise correctly to expected MCP field names.

**Traceability:**
- PR #76, PR #78 (registration wiring)
- Tests: `python/tests/test_server.py`, `python/tests/test_server_comprehensive.py`

---

### FR-MCP-003 — JSON-RPC Request Dispatch (tools/list, resources/list, prompts/list, tools/call)

**Title:** `Server.handle_request` dispatches MCP JSON-RPC method names to correct handlers.

**Description:** `handle_request(method, params)` is the single async entry point for all
MCP wire requests. It routes `tools/list`, `resources/list`, `prompts/list`, and
`tools/call` to the appropriate internal handler. Unknown methods return a
`-32601 Method not found` error object. Both sync and async tool handlers are supported
via `inspect.iscoroutinefunction`.

**Acceptance criteria:**
- `handle_request("tools/list", {})` returns `{"tools": [...]}`.
- `handle_request("resources/list", {})` returns `{"resources": [...]}`.
- `handle_request("prompts/list", {})` returns `{"prompts": [...]}`.
- `handle_request("tools/call", {"name": "x", "arguments": {}})` invokes the registered handler.
- `handle_request("unknown/method", {})` returns `{"jsonrpc": "2.0", "error": {"code": -32601, ...}}`.
- Both sync and async handlers execute correctly.

**Traceability:**
- PR #76, PR #80 (request validation hardening)
- Tests: `python/tests/test_server.py`, `python/tests/test_server_comprehensive.py`, `python/tests/test_integration.py`

---

### FR-MCP-004 — Request Validation + Structured JSON-RPC Errors

**Title:** Invalid requests return structured JSON-RPC error objects; the server never raises.

**Description:** `handle_request` validates that `method` is a non-empty string (`-32600`)
and that `params` is a dict or `None` (`-32600`). `_handle_tools_call` validates that
`name` is a non-empty string (`-32602`) and that `arguments` is a dict (`-32602`). An
unknown tool name returns `-32601`. Unhandled exceptions inside a handler are caught and
return `-32603 Internal error` (defensive guard). Every error response carries the
canonical `{"jsonrpc": "2.0", "error": {"code": N, "message": "...", "data": {...}}, "id": null}` shape.

**Acceptance criteria:**
- `handle_request(None, {})` → code `-32600`.
- `handle_request("tools/call", {"name": "", "arguments": {}})` → code `-32602`.
- `handle_request("tools/call", {"name": "x", "arguments": "bad"})` → code `-32602`.
- `handle_request("tools/call", {"name": "missing"})` → code `-32601`.
- Error response always has `jsonrpc`, `error.code`, `error.message`, and `id` fields.
- A tool handler that raises still returns an error dict (no exception propagated to caller).

**Traceability:**
- PR #80 (`[codex] harden MCP request validation`)
- Tests: `python/tests/test_server_comprehensive.py`, `python/tests/test_server.py`

---

### FR-MCP-005 — Governance Tool Bundle (ledger_query + ledger_verify)

**Title:** `governance_tools` bundle exposes `ledger_query` and `ledger_verify` MCP tools backed by Parpoura API.

**Description:** `python/src/pheno_mcp/tools/governance_tools.py` defines two tools:
`ledger_query` (GET `/api/v1/governance/ledger` with optional filters `from_entry`,
`to_entry`, `action`, `actor`, `workflow_id`, `limit`) and `ledger_verify`
(POST `/api/v1/governance/ledger/verify` with required `from_entry`/`to_entry`).
The Parpoura base URL is read from `PARPOURA_BASE_URL` env var (default `http://localhost:8001`).
HTTP errors are caught and returned as `{"error": ..., "status_code": ...}` — not raised.
`register_governance_tools(server)` wires both tools onto any `Server` instance.

**Acceptance criteria:**
- `ledger_query` input schema has zero required fields; all six filter fields are optional.
- `ledger_verify` input schema requires exactly `from_entry` and `to_entry`.
- `handle_ledger_query` issues `GET /api/v1/governance/ledger` and returns parsed JSON.
- `handle_ledger_verify` issues `POST /api/v1/governance/ledger/verify` and returns parsed JSON.
- HTTP 4xx/5xx errors return `{"error": ..., "status_code": N}` not an exception.
- `register_governance_tools(server)` makes both tools visible in `server.list_tools()`.

**Traceability:**
- PR #77 (wire ledger_query governance tool end-to-end)
- Tests: `python/tests/test_governance_tools.py`

---

### FR-MCP-006 — Session Tool Bundle (session_suspend + session_resume)

**Title:** `session_tools` bundle exposes `session_suspend` and `session_resume` MCP tools backed by Parpoura API.

**Description:** `python/src/pheno_mcp/tools/session_tools.py` defines:
`session_suspend` (POST `/api/v1/sessions/{session_id}/suspend` → `bundle_ref`) and
`session_resume` (POST `/api/v1/sessions/resume` with `bundle_ref` body → new `session_id`).
Both read `PARPOURA_BASE_URL`. HTTP errors are caught and returned as
`{"error": ..., "status_code": ...}`.

**Acceptance criteria:**
- `session_suspend` requires exactly `session_id`.
- `session_resume` requires exactly `bundle_ref`.
- `handle_session_suspend` issues `POST /api/v1/sessions/{session_id}/suspend`.
- `handle_session_resume` issues `POST /api/v1/sessions/resume` with `{"bundle_ref": ...}` body.
- HTTP errors returned as dict, not raised.
- `register_session_tools(server)` makes both tools visible in `server.list_tools()`.

**Traceability:**
- PR #78 (wire all tool bundles + `create_configured_server`)
- Tests: `python/tests/test_session_tools.py`

---

### FR-MCP-007 — Workflow Tool Bundle (workflow_execute / status / cancel / list)

**Title:** `workflow_tools` bundle exposes four workflow-lifecycle MCP tools backed by Parpoura API.

**Description:** `python/src/pheno_mcp/tools/workflow_tools.py` defines four tools:
`workflow_execute` (POST `/workflows/{id}/execute`),
`workflow_status` (GET `/workflows/{id}`),
`workflow_cancel` (POST `/workflows/{id}/cancel`),
`workflow_list` (GET `/workflows`).
`workflow_execute` accepts optional `workflow_type` (default `"default"`).
`workflow_status` reports lifecycle states `PENDING | RUNNING | SUSPENDED | COMPLETED | FAILED | CANCELLED`.
HTTP errors are caught and returned as `{"error": ..., "status_code": ...}`.

**Acceptance criteria:**
- `workflow_execute` requires `workflow_id`; `workflow_type` is optional.
- `workflow_status` and `workflow_cancel` each require exactly `workflow_id`.
- `workflow_list` requires no arguments.
- Each handler issues the correct HTTP verb + path to `PARPOURA_BASE_URL`.
- HTTP errors returned as dict, not raised.
- `register_workflow_tools(server)` makes all four tools visible in `server.list_tools()`.

**Traceability:**
- PR #78
- Tests: `python/tests/test_workflow_tools.py`

---

### FR-MCP-008 — `create_configured_server` Factory

**Title:** `create_configured_server(config?)` returns a fully-wired `Server` with all 19 tools pre-registered.

**Description:** `server.py::create_configured_server` is a zero-boilerplate factory that
instantiates a `Server` and calls `register_governance_tools`, `register_agent_tools`,
`register_knowledge_tools`, `register_policy_tools`, `register_session_tools`, and
`register_workflow_tools` in sequence. This gives callers a ready-to-use server with
all 19 tools (`ledger_query`, `ledger_verify`, `agent_create`, `agent_list`,
`agent_get`, `agent_delete`, `knowledge_store`, `knowledge_retrieve`,
`knowledge_search`, `knowledge_delete`, `policy_list`, `policy_get`,
`policy_evaluate`, `session_suspend`, `session_resume`, `workflow_execute`,
`workflow_status`, `workflow_cancel`, `workflow_list`) without
importing individual bundle modules.

**Acceptance criteria:**
- `create_configured_server()` returns a `Server` with exactly 19 tools registered.
- `create_configured_server(ServerConfig(name="test"))` propagates the config to the server.
- Returned server handles `tools/list` and returns all 19 tool names.
- No exception raised on construction with default config.

**Traceability:**
- PR #78 (`feat(tools): wire all tool bundles + create_configured_server factory`)
- Tests: `python/tests/test_configured_server.py`

---

### FR-MCP-009 — SearchPort Hexagonal Port Trait (Rust)

**Title:** Object-safe `SearchPort` trait with `ensure_index / index_documents / search / delete_document`.

**Description:** `crates/pheno-ports/src/lib.rs` defines `SearchPort` as an async
object-safe Rust trait (`Box<dyn SearchPort>` compiles). Methods: `ensure_index(index, pk)`,
`index_documents(index, docs)`, `search(index, query) -> SearchResults`,
`delete_document(index, id)`. Domain types `SearchDocument` (id + flattened fields) and
`SearchResults` (hits, estimated_total_hits, processing_time_ms, query) are serde-serialisable.
`SearchPortError` variants: `Index`, `Search`, `Delete`, `Transport`.

**Acceptance criteria:**
- `Box<dyn SearchPort>` compiles (object-safety smoke test passes).
- `InMemorySearchStore` in `doubles` module fully implements the trait.
- `ensure_index` is idempotent — calling twice on the same index name succeeds.
- `index_documents` stores all documents; subsequent `search` finds them by substring.
- `delete_document` removes the document; subsequent search finds nothing.
- `SearchResults.estimated_total_hits` matches the actual hit count.

**Traceability:**
- PR #81 (`feat: hexagonal port traits — SearchPort + SkillStoragePort (audit #4)`)
- Tests: `crates/pheno-ports/src/doubles.rs` (test series `search_double_*`)

---

### FR-MCP-012 — pheno-qdrant SearchPort Adapter (SHIPPED)

**Title:** `QdrantClient` implements `SearchPort` via thin REST client; no live server required for unit tests.

**Description:** `crates/pheno-qdrant/src/lib.rs` provides `QdrantClient::new(url, api_key)` which
implements `SearchPort` by bridging Qdrant's REST API:

| `SearchPort` method   | Qdrant REST call                                      |
|-----------------------|-------------------------------------------------------|
| `ensure_index`        | `PUT /collections/{index}` (idempotent, 200/409 OK)   |
| `index_documents`     | `PUT /collections/{index}/points` (zero-vector + payload) |
| `search`              | `POST /collections/{index}/points/scroll` (payload `match.text` filter) |
| `delete_document`     | `POST /collections/{index}/points/delete` (by `_pheno_id` filter) |

Request-body construction (`build_create_collection_body`, `build_upsert_body`,
`build_scroll_body`, `build_delete_body`) is extracted as pure functions and
fully unit-tested without a live Qdrant.  Live-server tests are gated behind
`#[ignore = "requires live Qdrant on localhost:6333"]`.

**Acceptance criteria:**
- `Box<dyn SearchPort> = Box::new(QdrantClient::new(...))` compiles (object-safety).
- `build_upsert_body` serialises `SearchDocument.id` as both point id and `_pheno_id` payload field.
- `build_scroll_body` emits a `should` filter with `match.text` clauses covering `name`, `content`, `text`, `_pheno_id`.
- `build_delete_body` targets the `_pheno_id` payload field.
- `cargo test -p pheno-qdrant` passes 8 unit tests; 3 live tests are `#[ignore]`d.
- `cargo check --workspace` is clean (no errors, no warnings).

**Traceability:**
- PLAN-MCP-002 → SHIPPED
- PR feat/qdrant-searchport
- Tests: `crates/pheno-qdrant/src/lib.rs` (`test_build_*`, `test_qdrant_client_is_search_port`)

---

### FR-MCP-010 — SkillStoragePort Hexagonal Port Trait (Rust)

**Title:** Object-safe `SkillStoragePort` trait with `put / get / list / delete` for `SkillEntry` records.

**Description:** `crates/pheno-ports/src/lib.rs` defines `SkillStoragePort` as an async
object-safe trait. `SkillEntry` fields: `id`, `name`, `version`, `code`, `runtime`,
`metadata (serde_json::Value)`. Methods: `put(entry) -> SkillEntry`, `get(id) -> SkillEntry`,
`list() -> Vec<SkillEntry>`, `delete(id)`. `StoragePortError` variants: `NotFound`,
`Serialise`, `Backend`.

**Acceptance criteria:**
- `Box<dyn SkillStoragePort>` compiles (object-safety smoke test passes).
- `InMemorySkillStore` in `doubles` module fully implements the trait.
- `put` then `get` returns bit-identical entry.
- `put` on an existing id replaces the entry (upsert semantics).
- `get` on a missing id returns `StoragePortError::NotFound`.
- `list` returns all previously `put` entries.
- `delete` removes the entry; subsequent `get` returns `NotFound`.

**Traceability:**
- PR #81
- Tests: `crates/pheno-ports/src/doubles.rs` (test series `skill_store_*`),
  `crates/phenotype-surrealdb/src/lib.rs` (test series `test_port_*`, `test_pheno_surreal_*`)

---

### FR-MCP-014 — phenotype-surrealdb SkillStoragePort Adapter (SHIPPED)

**Title:** `PhenoSurreal` implements `SkillStoragePort` via real SurrealDB local engines with schema bootstrap.

**Description:** `crates/phenotype-surrealdb/src/lib.rs` provides `PhenoSurreal::new(path)`
which connects to `surrealdb::Surreal<surrealdb::engine::local::Db>`, selects a fixed
namespace/database, and bootstraps `skills` as a `SCHEMAFULL` table. `path` controls the
embedded engine: `mem://` selects `Mem`, while all other paths use `SurrealKv`. Legacy
helpers `store_skill`, `query_skills`, and `store_embedding` are preserved for callers
that pre-date the port trait. Fast unit tests use `mem://`; ignored tests cover the live
SurrealKv path.

**Acceptance criteria:**
- `PhenoSurreal::new("mem://").await` succeeds without external services.
- `Box<dyn SkillStoragePort> = Box::new(PhenoSurreal::new(...).await.unwrap())` compiles.
- `SkillStoragePort::put/get/list/delete` work through the SurrealDB API.
- `skills` is created as a `SCHEMAFULL` table with typed fields.
- Ignored tests exercise the local SurrealKv path for round-trip and delete coverage.

**Traceability:**
- PLAN-MCP-001 → SHIPPED
- Tests: `crates/phenotype-surrealdb/src/lib.rs` (all non-ignored `#[tokio::test]` functions)

---

### FR-MCP-013 — MCP Transport Binding (stdio / HTTP) — SHIPPED

**Title:** `build_fastmcp_bridge` + `run_stdio` expose the configured server over a real MCP transport so any MCP client can connect.

**Description:** `python/src/pheno_mcp/transport.py` provides:

- `build_fastmcp_bridge(server)` — wraps a pheno_mcp `Server` in a `FastMCP`
  instance.  Every registered `Tool` is re-exposed on the `FastMCP` server as
  an async handler that delegates to `server.handle_request("tools/call", ...)`.
  FastMCP handles all wire framing (JSON-RPC init, capabilities negotiation,
  `tools/list` advertising, request routing).
- `run_stdio(config?)` — synchronous convenience function that builds a fully
  configured server and calls `FastMCP.run(transport="stdio")`.  This is the
  function invoked by `python -m pheno_mcp`.
- `run_stdio_async(config?)` — async variant for embedding in test harnesses.

`python/src/pheno_mcp/__main__.py` provides the `main()` entry point so the
server is launchable as `python -m pheno_mcp`.  The `pyproject.toml`
`[project.scripts]` table registers a `pheno-mcp` console script pointing at
`pheno_mcp.__main__:main`.

The `mcp>=1.27` dependency is declared in `pyproject.toml` and resolved via
`uv.lock`.  The `mcp` import is lazy (inside `build_fastmcp_bridge`) to avoid
import-time segfaults on Python 3.14 alpha builds where pydantic-core's C
extension has an ABI mismatch.

**Acceptance criteria:**
- `from pheno_mcp.transport import build_fastmcp_bridge, run_stdio, run_stdio_async` succeeds.
- `build_fastmcp_bridge(create_configured_server())` registers all 19 tools on a FastMCP instance.
- `run_stdio` is a synchronous callable; `run_stdio_async` is an async coroutine function.
- `python -m pheno_mcp` entrypoint imports and exposes `main()`.
- `build_fastmcp_bridge, run_stdio, run_stdio_async` are re-exported from `pheno_mcp.__init__`.
- On Python builds where `mcp` is importable: an in-memory `ClientSession` can call `list_tools` and `call_tool` end-to-end via the bridge.

**Traceability:**
- PLAN-MCP-006 → SHIPPED
- PR feat/mcp-transport
- Tests: `python/tests/test_transport.py`

---

### FR-MCP-015 — Additional Parpoura Tool Bundles (agent, knowledge, policy)

**Title:** `agent_tools`, `knowledge_tools`, and `policy_tools` expose 11 additional MCP tools backed by Parpoura API.

**Description:** `python/src/pheno_mcp/tools/agent_tools.py`,
`python/src/pheno_mcp/tools/knowledge_tools.py`, and
`python/src/pheno_mcp/tools/policy_tools.py` define:
`agent_create`, `agent_list`, `agent_get`, `agent_delete`;
`knowledge_store`, `knowledge_retrieve`, `knowledge_search`, `knowledge_delete`; and
`policy_list`, `policy_get`, `policy_evaluate`.
The bundles follow the same `PARPOURA_BASE_URL` env-var pattern as the existing tool modules and return HTTP failures as `{"error": ..., "status_code": ...}` instead of raising.
`register_agent_tools(server)`, `register_knowledge_tools(server)`, and `register_policy_tools(server)` wire all 11 tools onto any `Server` instance.

**Acceptance criteria:**
- `agent_create` uses `POST /api/v1/agents`; `agent_list` uses `GET /api/v1/agents`; `agent_get` uses `GET /api/v1/agents/{agent_id}`; `agent_delete` uses `DELETE /api/v1/agents/{agent_id}`.
- `knowledge_store` uses `POST /api/v1/knowledge`; `knowledge_retrieve` uses `GET /api/v1/knowledge/{knowledge_id}`; `knowledge_search` uses `GET /api/v1/knowledge/search`; `knowledge_delete` uses `DELETE /api/v1/knowledge/{knowledge_id}`.
- `policy_list` uses `GET /api/v1/policies`; `policy_get` uses `GET /api/v1/policies/{policy_id}`; `policy_evaluate` uses `POST /api/v1/policies/evaluate`.
- Each handler returns parsed JSON on success and `{"error": ..., "status_code": N}` for HTTP failures.
- `register_agent_tools(server)`, `register_knowledge_tools(server)`, and `register_policy_tools(server)` make all 11 tools visible in `server.list_tools()`.

**Traceability:**
- PLAN-MCP-004 → SHIPPED
- Tests: `python/tests/test_agent_tools.py`, `python/tests/test_knowledge_tools.py`, `python/tests/test_policy_tools.py`

---

## Non-Functional Requirements

### NFR-MCP-001 — Object-Safe Port Traits

**Title:** All hexagonal port traits in `pheno-ports` must be usable as `Box<dyn Trait>`.

**Description:** Rust's object-safety rules require that async methods use `async_trait`
macro and that no associated-type ambiguity exists. Both `SearchPort` and `SkillStoragePort`
carry the `#[async_trait]` attribute. Object-safety is verified by a compilation smoke test
in each crate's test suite: `let _: Box<dyn SearchPort> = Box::new(InMemorySearchStore::new())`.

**Evidence:** `search_double_is_object_safe` and `skill_store_is_object_safe` tests in
`crates/pheno-ports/src/doubles.rs`; `test_pheno_surreal_is_skill_storage_port` in
`crates/phenotype-surrealdb/src/lib.rs`.

---

### NFR-MCP-002 — FastMCP / MCP Convention Compliance

**Title:** Python package follows FastMCP naming and structural conventions for compatibility with MCP clients.

**Description:** Tool `input_schema` is returned under the key `inputSchema` (camelCase)
per MCP wire format. Resource `mime_type` is serialised as `mimeType`. Handler dispatch
detects async callables via `inspect.iscoroutinefunction` to avoid blocking the event loop.
`PARPOURA_BASE_URL` is an env-var override, not a hard-coded URL, matching Phenotype
config-via-env policy.

**Evidence:** `Server.to_dict()` in `server.py`; async dispatch in `_handle_tools_call`;
`governance_tools._client()` env-var pattern replicated in all three tool bundles.

---

### NFR-MCP-003 — Errors Never Crash the Server

**Title:** No unhandled exception from a tool handler may propagate past `handle_request`.

**Description:** `_handle_tools_call` wraps handler invocation in a `try/except Exception`
guard. Any exception returns a `-32603 Internal error` JSON-RPC object. The server process
continues normally. This rule applies to both sync and async handlers.

**Evidence:** `server.py` lines 294–298 (defensive guard); `python/tests/test_server_comprehensive.py`
covers handler-exception path.

---

### NFR-MCP-004 — Test Suite Coverage (279 Tests Green)

**Title:** All 279 tests (260 Python + 19 Rust) pass on every merge to `main`.

**Description:** CI must run `uv run pytest python/` (260 passing, 2 skipped on Python 3.14a) and
`cargo test --workspace` (≥ 19 passing) on every PR. Failures block merge.
Test files trace to at least one FR via naming convention (`test_governance_tools.py` → FR-MCP-005, etc.).

**Evidence:** `uv run pytest` output: `260 passed, 2 skipped in 2.94s`; `cargo test --workspace` output:
`19 passed` across `pheno-ports` (11) + `phenotype-surrealdb` (5) + `pheno-meilisearch` (2) + `pheno_mcp` crate (1).

---

### NFR-MCP-005 — SurrealDB Local-Engine Wiring

**Title:** `phenotype-surrealdb` uses real SurrealDB local engines with a clear mem-vs-persistent split.

**Description:** `phenotype-surrealdb/src/lib.rs` now connects through
`surrealdb::Surreal<surrealdb::engine::local::Db>`, bootstraps the `skills` table as
`SCHEMAFULL`, and routes `mem://` to `Mem` while all other paths use `SurrealKv`.
This keeps unit tests fast and deterministic while still supporting a persistent local
engine path for ignored live coverage.

**Evidence:** `crates/phenotype-surrealdb/src/lib.rs` constructor and CRUD implementation;
`crates/phenotype-surrealdb/src/lib.rs` ignored live SurrealKv tests; `surrealdb` local
engine features in `crates/phenotype-surrealdb/Cargo.toml`.

---

### NFR-MCP-006 — CI Security Baseline (SHA-Pinned Actions, ubuntu-24.04)

**Title:** All GitHub Actions workflows use SHA-pinned action references on ubuntu-24.04 runners.

**Description:** Per Phenotype org governance, all `uses:` references in `.github/workflows/`
must be pinned to full commit SHAs. The main CI pipeline targets `ubuntu-24.04` (not
`ubuntu-latest`) for reproducibility. TruffleHog secret scanning and Scorecard action
are required CI steps.

**Evidence:** PR #76 (`chore: PhenoMCP workflow hygiene ubuntu-24.04 + SHA pins`);
`.github/workflows/` files contain SHA-format action pins.

---

## Test-ID to Catalog Mapping

| Test file / series | Catalog FR/NFR |
|---|---|
| `test_init.py` | FR-MCP-001 |
| `test_server.py`, `test_server_comprehensive.py` | FR-MCP-002, FR-MCP-003, FR-MCP-004, NFR-MCP-003 |
| `test_integration.py` | FR-MCP-003 |
| `test_governance_tools.py` | FR-MCP-005 |
| `test_agent_tools.py` | FR-MCP-015 |
| `test_knowledge_tools.py` | FR-MCP-015 |
| `test_policy_tools.py` | FR-MCP-015 |
| `test_session_tools.py` | FR-MCP-006 |
| `test_workflow_tools.py` | FR-MCP-007 |
| `test_configured_server.py` | FR-MCP-008 |
| `test_client.py`, `test_client_connected.py` | FR-MCP-001 (Client symbol) |
| `test_transport.py` | FR-MCP-013 |
| `test_models.py`, `test_models_edge_cases.py` | FR-MCP-002 (domain types) |
| `doubles.rs :: search_double_*` | FR-MCP-009, NFR-MCP-001 |
| `pheno-qdrant :: test_build_*`, `test_qdrant_client_is_search_port` | FR-MCP-012, NFR-MCP-001 |
| `doubles.rs :: skill_store_*` | FR-MCP-010, NFR-MCP-001 |
| `phenotype-surrealdb :: test_*` | FR-MCP-010, FR-MCP-014, NFR-MCP-001, NFR-MCP-005 |

---

## Gaps / PLANNED

| ID | Title | Notes |
|---|---|---|
| ~~PLAN-MCP-001~~ | ~~Real SurrealDB client wiring~~ | **SHIPPED** → FR-MCP-014; `PhenoSurreal` now wires `surrealdb::Surreal` with `Mem` / `SurrealKv`, schema bootstrap, and CRUD via the SurrealDB API |
| ~~PLAN-MCP-002~~ | ~~pheno-qdrant SearchPort adapter~~ | **SHIPPED** → FR-MCP-012; `QdrantClient` implements `SearchPort` via thin REST; 8 unit tests green |
| PLAN-MCP-003 | pheno-meilisearch SearchPort adapter | `crates/pheno-meilisearch/` exists (stub); needs `SearchPort` impl backed by `meilisearch-sdk` |
| ~~PLAN-MCP-004~~ | ~~Additional Parpoura tool bundles~~ | **SHIPPED** → FR-MCP-015; agent, knowledge, and policy tool bundles are wired end-to-end with 11 tools total |
| PLAN-MCP-005 | MCP ↔ Claude SDK contract hardening | No schema-level validation between MCP request shape and SDK client expectations; property-based tests needed |
| ~~PLAN-MCP-006~~ | ~~Transport layer (stdio / HTTP / WS)~~ | **SHIPPED** → FR-MCP-013; `build_fastmcp_bridge` wraps configured Server over FastMCP stdio transport; `python -m pheno_mcp` entrypoint; 8 unit tests green |
| PLAN-MCP-007 | Resource + Prompt handler end-to-end | Registration is wired; no Parpoura-backed handlers exist for `resources/read` or `prompts/get` |
| PLAN-MCP-008 | FR coverage matrix auto-update | `docs/reference/fr_coverage_matrix.md` is empty; auto-population from test annotations needed |

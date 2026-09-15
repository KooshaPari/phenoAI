# Substrate MCP Servers — Specification

**Service Group:** Substrate (MCP routing/middleware layer)
**Location:** `servers/substrate/`
**Framework:** PhenoFastMCP / fastmcp 3.4.2
**Maturity Score:** 3/10

The substrate server group forms a **routing/middleware layer** between MCP clients and the OmniRoute backend. It provides dispatch tools, mailbox (team inbox) operations, and health-checking — adding minimal processing overhead on top of upstream LLM calls.

| Module | Catalog ID | Role |
|---|---|---|
| `substrate_server.py` | `substrate` | Substrate dispatch server (HTTP, argv, cheap-llm CLI) |
| `dispatch_server.py` | `substrate-dispatch` | Thin wrapper around dispatch_mcp |
| `dispatch_mcp/` | — | FastMCP server exposing `dispatch_*` tools, delegating to OmniRoute HTTP backend |
| `lead_server.py` | (mailbox variant) | Mailbox leader — team inbox coordination |
| `team_mailbox_server.py` | (worker mailbox) | Team worker inbox |

---

## 1. Hexagonal Architecture — Port / Adapter Map

### Current State

#### Existing Ports & Adapters (correct)

| Layer | Element | File | Role |
|---|---|---|---|
| **Domain** | `JobResult` | `dispatch_mcp/core/types.py:7-28` | Domain type: holds job ID, status, output, error |
| **Domain (port)** | `Router` Protocol | `dispatch_mcp/core/port.py:7-22` | Outbound port: `dispatch()`, `health()`, `cancel()`, `close()` |
| **Adapter** | `OmniHttpAdapter` | `dispatch_mcp/adapters/omni_http.py:8-46` | Implements `Router` via httpx to OmniRoute backend |
| **DI** | `_RouterHolder` | `dispatch_mcp/server.py:42-64` | Setter-injection singleton (enables test substitution) |

#### Where Core Lives

```
dispatch_mcp/core/
  port.py       ← Router Protocol (interface owned by core) ✓
  types.py      ← JobResult domain type in core             ✓
```

#### Missing Ports (critical gaps)

| Missing Port | Current (leaky) | File | Problem |
|---|---|---|---|
| **Inbound port** | MCP tools as module-level closures via `@mcp.tool()` | `dispatch_mcp/server.py` | No `DispatchService` interface; tools cannot be tested independently of FastMCP |
| **HttpClient port** | Module-level free function | `_http.py` | Direct `httpx.get()`/`httpx.post()` with no abstraction |
| **Repository port** | Direct SQLite calls | `_db.py` | `sqlite3` queries mixed into server code; no `MailboxRepository` or `TaskRepository` interface |
| **HealthChecker port** | `health.py` calls `httpx.get()` directly | `dispatch_mcp/health.py` | Cannot test health logic without a real HTTP endpoint |
| **Config port** | `os.environ` reads scattered across modules | Various | No validated `Config` dataclass; implicit env-var coupling |
| **ResponseSanitizer port** | Module-level free function | `_sanitize.py` | No interface; cannot swap or test sanitization strategy |

#### Direction Violations

| Violation | Detail |
|---|---|
| `_http.py` → httpx | HTTP client has no abstraction layer |
| `_db.py` → sqlite3 | Database access has no repository abstraction |
| `health.py` → httpx.get() | Health checking has no port; requires real endpoint for testing |

---

## 2. Dependency Direction

### Current (simplified)

```
┌──────────────────────────────────────────────────────────────┐
│                    FastMCP (transport layer)                  │
└──────────────────────┬───────────────────────────────────────┘
                       │ @mcp.tool() closures
                       ▼
┌──────────────────────────────────────────────────────────────┐
│                     server.py (module-level)                  │
│                                                                │
│  _RouterHolder.get() → OmniHttpAdapter (implements Router)    │
│  _http.*              → httpx (direct — no abstraction)       │
│  _db.*                → sqlite3 (direct — no abstraction)     │
│  _sanitize.*          → module-level (no abstraction)         │
│  health.*             → httpx.get() (direct)                  │
└──────────────────────────────────────────────────────────────┘
```

### Target (hexagonal — all dependencies point inward)

```
                    ┌───────────────────┐
                    │   FastMCP (MCP)   │  ── transport (infrastructure)
                    └─────────┬─────────┘
                              │ calls inbound port
                              ▼
┌──────────────────────────────────────────────────────────────┐
│                    Inbound Port (interface)                    │
│  DispatchService  MailboxService  HealthService  Config      │
└──────────────────────────┬───────────────────────────────────┘
                           │ implemented by
                           ▼
┌──────────────────────────────────────────────────────────────┐
│                    Application / Domain                       │
│  dispatch_mcp/core/                                          │
│    port.py ── Router Protocol (outbound port)                │
│    types.py ── JobResult (domain type)                       │
└──────────────────────────┬───────────────────────────────────┘
                           │ calls outbound ports
                           ▼
┌──────────────────────────────────────────────────────────────┐
│                    Outbound Ports (interfaces)                 │
│  Router  HttpClient  Repository  HealthChecker               │
│  ResponseSanitizer  ConfigProvider                            │
└──────────────────────────┬───────────────────────────────────┘
                           │ implemented by
                           ▼
┌──────────────────────────────────────────────────────────────┐
│                    Adapters (infrastructure)                   │
│  OmniHttpAdapter  HttpxClient  SqliteMailboxRepo             │
│  EnvConfig  Sanitizer  HttpHealthChecker                     │
└──────────────────────────────────────────────────────────────┘
```

**Rule:** Dependencies point _inward_. Domain/core knows nothing about FastMCP, httpx, sqlite3, or environment variables. Adapters implement ports; the composition root wires them together.

---

## 3. Latency Budgets

### Operation Budgets (at p99)

| Operation | Total Budget | Substrate Overhead | Notes |
|---|---|---|---|
| `dispatch_*` (any tier) | < 15s end-to-end | < 100ms | Dominated by upstream LLM response |
| `dispatch_custom` | < 15s end-to-end | < 100ms | Same as above |
| `dispatch_health` | < 5s | < 500ms | Health check against OmniRoute backend |
| `dispatch_liveness` | < 100ms | < 10ms | In-process, no external dependency |
| `substrate_dispatch` | < 30s | < 100ms | Substrate HTTP may queue/spawn engines |
| `team_send` | < 500ms | < 50ms | Local SQLite insert |
| `team_inbox` | < 1s | < 100ms | Local SQLite query |
| Server startup | < 5s | < 5s | Cold start includes importing modules |

### Component-Level Budget (p99)

| Component | Budget | Criticality |
|---|---|---|
| Request parsing (FastMCP overhead) | < 5ms | High |
| Tier validation | < 1ms | High |
| Message size validation | < 1ms | High |
| HTTP connection pool acquire | < 10ms | Medium |
| HTTP request serialization + send | < 25ms | Medium |
| HTTP response receive + parse | < 25ms | Medium |
| Response sanitization | < 1ms | High |
| Response serialization (FastMCP) | < 5ms | High |
| **Total substrate overhead** | **< 73ms** | |

---

## 4. SLO Targets

| Metric | Target | Measurement Method |
|---|---|---|
| `dispatch_liveness` success rate | 100% | Uptime monitoring (process health check) |
| `dispatch_*` availability | 99.9% | Depends on OmniRoute upstream; composite SLO |
| Dispatch overhead (p99) | < 100ms | Custom metric from process start/end timestamps |
| Response sanitization completeness | 100% | Property-based test: no internal keys leak in responses |
| Server uptime | 99.99% | Process health check / watchdog |
| MCP tool response time (p99) | < 100ms overhead | Histogram metric (Prometheus) |

---

## 5. Improvement Roadmap

### P0 — Critical

- [ ] **Formalize inbound port** — Extract MCP tool handlers into a `DispatchService` interface so tools can be tested independently of FastMCP. Currently tools are module-level closures registered via `@mcp.tool()`.
- [ ] **Add repository ports** — Introduce `MailboxRepository` and `TaskRepository` protocols to abstract `_db.py` SQLite calls. Enables test doubles for database-backed operations (`team_send`, `team_inbox`).

### P1 — High

- [ ] **Abstract HTTP client into port** — Define an `HttpClient` protocol and move `_http.py` into an adapter implementing it. Removes direct httpx dependency from business logic.
- [ ] **Formalize configuration port** — Replace ad-hoc `os.environ` reads with a validated `Config` dataclass. All config access flows through a single `ConfigProvider` port.
- [ ] **Extract health checking into port** — Add a `HealthChecker` protocol so `dispatch_mcp/health.py` depends on an interface, not on `httpx.get()` directly.

### P2 — Medium

- [ ] **Create composition root** — Add a wiring/DI module (e.g., `dispatch_mcp/composition.py`) that wires all ports to adapters. Eliminates scattered `_RouterHolder`-style singletons.
- [ ] **Add ResponseSanitizer port** — Abstract `_sanitize.py` behind a `ResponseSanitizer` protocol. Enables strategy swapping and isolated testing.

### P3 — Low

- [ ] **Add Prometheus histograms** — Instrument dispatch latency at each component boundary (request parsing, HTTP send, response sanitization) to validate latency budgets empirically.

---

*Generated from hexagonal architecture audit — 2026-06-20. Maturity score: 3/10.*

"""Tool-discovery and invalid-call rejection tests for the substrate MCP server.

Traces to: audit/2026-09-05 PhenoMCPServers.
  "Validate catalog against schemas and packaged entrypoints;
   prove tool discovery and invalid-call rejection for named server;
   check dependency pin resolves intended hard fork"

These tests lock the **runtime contract** that the catalog (`catalog/registry.yaml`)
claims `substrate` satisfies:

  - `framework: fastmcp`  — server boots under fastmcp
  - `transport: stdio`    — server is invokable as a Python module (the
                            catalog's `entry: {module: substrate_server, callable: mcp.run}`
                            resolves to the `mcp` global in that module)
  - The 6 tools declared in the source (`substrate_dispatch`,
    `substrate_plan`, `substrate_route`, `team_send`, `team_inbox`,
    `task_list`) are all registered and discoverable via
    `FastMCP.list_tools()` — the same call a real MCP client would
    make to populate its tool menu.

  - Invalid calls do NOT crash the server:
    * `substrate_dispatch({"prompt": ""})` returns the documented
      `{"error":"prompt must not be empty"}` envelope.
    * `call_tool("nonexistent_tool", {})` raises `NotFoundError` —
      the framework's standard rejection (no silent pass-through).
    * `substrate_route({"task": "not a dict"})` is rejected by
      Pydantic's schema validator on the tool signature, not by
      a late runtime check. This pins the contract: callers that
      know the schema get an immediate, structured failure.

These are the narrowest useful acceptance tests for the audit's
"tool discovery and invalid-call rejection" clause. The full tool
*semantics* (dispatch HTTP routes, mailbox DB writes) are covered by
the existing substrate tests in `servers/substrate/tests/`; this
file covers the **boundary contract** the catalog advertises.
"""
from __future__ import annotations

import sys
from pathlib import Path

# Import path setup mirrors conftest.py for the local `tests/` dir.
# `substrate_server` is a top-level module in `servers/substrate/`,
# not a package — `import substrate_server` only works when that
# directory is on sys.path. The catalog CI sets `PYTHONPATH=.` and
# runs from the repo root, where `servers/substrate/` is NOT a
# direct import path. So we add it explicitly here; the validator
# also needs to handle this.
_SUBSTRATE_DIR = Path(__file__).resolve().parent.parent / "servers" / "substrate"
if str(_SUBSTRATE_DIR) not in sys.path:
    sys.path.insert(0, str(_SUBSTRATE_DIR))


import pytest

# Importing the substrate_server module loads the FastMCP instance
# and registers all `@mcp.tool()` functions. If fastmcp is not
# installed the import will fail — we surface a clear skip rather
# than an opaque collection error so the catalog CI signal is
# actionable.
try:
    import substrate_server  # noqa: F401  (import side-effect: mcp.tool() registration)
    from fastmcp.exceptions import NotFoundError
except ImportError as exc:  # pragma: no cover - exercised by CI matrix
    pytest.skip(
        f"substrate_server or fastmcp not importable: {exc}",
        allow_module_level=True,
    )


# ---------------------------------------------------------------------------
# Tool discovery — the catalog advertises 6 tools on `substrate`
# ---------------------------------------------------------------------------

EXPECTED_TOOLS = {
    "substrate_dispatch",
    "substrate_plan",
    "substrate_route",
    "team_send",
    "team_inbox",
    "task_list",
}


def test_substrate_fastmcp_instance_is_named_substrate() -> None:
    """The FastMCP global must be named 'substrate' so stdio transport
    discovery (`mcp.run()`) announces the right server identity to
    MCP clients."""
    assert substrate_server.mcp.name == "substrate"


def test_substrate_registers_exactly_the_expected_tools() -> None:
    """The 6 tools are the contract — adding/removing a tool is a
    breaking catalog change and must show up in this test before
    it ships to a consumer."""
    import asyncio
    tools = asyncio.run(substrate_server.mcp.list_tools())
    actual = {t.name for t in tools}
    assert actual == EXPECTED_TOOLS, (
        f"Tool set drift. expected={EXPECTED_TOOLS} actual={actual} "
        f"missing={EXPECTED_TOOLS - actual} extra={actual - EXPECTED_TOOLS}"
    )


def test_each_tool_has_a_non_empty_description() -> None:
    """Empty descriptions break MCP clients that render tooltips
    from `tool.description`. Pin against silent regressions where
    a docstring is removed during a refactor."""
    import asyncio
    tools = asyncio.run(substrate_server.mcp.list_tools())
    for t in tools:
        assert t.description and t.description.strip(), (
            f"tool {t.name!r} has empty/blank description"
        )


# ---------------------------------------------------------------------------
# Invalid-call rejection — three failure shapes, each pinned
# ---------------------------------------------------------------------------


def test_substrate_dispatch_rejects_empty_prompt_with_documented_error() -> None:
    """Empty prompt must return the documented error envelope, not
    raise, not crash, and not silently pass through to HTTP."""
    import asyncio
    result = asyncio.run(
        substrate_server.mcp.call_tool(
            "substrate_dispatch", {"prompt": ""}
        )
    )
    # FastMCP returns a CallToolResult with structured_content; for
    # an envelope-style tool it should contain the error key.
    payload = getattr(result, "structured_content", None) or {}
    if not payload and result.content:
        # Fallback: parse the JSON text content.
        import json
        payload = json.loads(result.content[0].text)
    assert payload == {"error": "prompt must not be empty"}, payload


def test_substrate_dispatch_rejects_whitespace_only_prompt() -> None:
    """Whitespace-only is the same as empty after `prompt.strip()` —
    pin the predicate behavior so callers can't bypass the check
    with `'   '`."""
    import asyncio
    result = asyncio.run(
        substrate_server.mcp.call_tool(
            "substrate_dispatch", {"prompt": "   \n\t  "}
        )
    )
    payload = getattr(result, "structured_content", None) or {}
    if not payload and result.content:
        import json
        payload = json.loads(result.content[0].text)
    assert payload == {"error": "prompt must not be empty"}, payload


def test_call_unknown_tool_raises_not_found_error() -> None:
    """Unknown tool name must surface as NotFoundError so MCP
    clients see a structured rejection, not a 200-OK with an
    opaque string error. This pins the framework's contract:
    callers must NOT silently receive a different tool's result."""
    import asyncio
    with pytest.raises(NotFoundError):
        asyncio.run(
            substrate_server.mcp.call_tool("definitely_not_a_real_tool", {})
        )


def test_substrate_route_rejects_non_dict_task_with_validation_error() -> None:
    """`substrate_route(task: dict)` is schema-validated; passing
    a string must fail at the Pydantic boundary (immediate,
    structured) — not at the server's runtime `isinstance(task, dict)`
    check, which would surface a different error shape. This pins
    the *framework-level* rejection contract.

    Note: FastMCP re-raises pydantic.ValidationError as
    fastmcp.exceptions.ValidationError. Either form should fail
    the call — both signal "bad arguments, not a server bug".
    """
    import asyncio
    from fastmcp.exceptions import ValidationError as FastMCPValidationError

    with pytest.raises(FastMCPValidationError):
        asyncio.run(
            substrate_server.mcp.call_tool(
                "substrate_route", {"task": "not a dict"}
            )
        )

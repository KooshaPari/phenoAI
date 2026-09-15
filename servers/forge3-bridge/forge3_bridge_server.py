"""forge3_bridge — local MCP bridge to the Forge Agent SDK (forge3).

Forge3 is a Rust binary that exposes a JSON-RPC 2.0 surface over stdio or
WebSocket (`ws://127.0.0.1:9753`).  This server wraps the JSON-RPC surface
as MCP tools so any MCP-aware client (Claude Desktop, Codex, Cursor) can
call into the Forge Agent SDK.

Tools exposed (every call translates to one forge3 RPC; results are forwarded
verbatim after flattening forge3's `result.data.complete.X` envelopes):

    forge3_info()                          -> rpc.info
    forge3_methods()                       -> rpc.discover (method list)
    forge3_tools()                         -> tool_list
    forge3_extensions()                    -> info.extensions
    forge3_models()                        -> model_list
    forge3_agents()                        -> agent_list
    forge3_commands()                      -> command_list
    forge3_call(method, params_json?)      -> raw JSON-RPC passthrough
    forge3_shell(command, cwd?)            -> tool.shell wrapper
    forge3_search(pattern, path?, mode?)   -> tool.search wrapper
    forge3_read(path, max_lines?)          -> tool.read wrapper
    forge3_write(path, content)            -> tool.write wrapper
    forge3_patch(path, old, new, replace_all?) -> tool.patch wrapper
    forge3_skill_search(query)             -> tool.skill_search wrapper
    forge3_doctor()                        -> ws + stdio transport health

Run:

    # As an MCP server (stdio):
    python -m forge3_bridge_server

    # Or, after `forge3-ctl install`:
    forge3-bridge-mcp
"""
from __future__ import annotations

import json
import os
import sys
from typing import Any, Optional

# Reuse the CLI's transport layer so both front-ends share behavior.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from forge3_cli import (  # noqa: E402
    DEFAULT_BIN,
    DEFAULT_WS,
    ForgeRpcError,
    StdioTransport,
    Transport,
    TransportError,
    flatten_result,
    pick_transport,
    render,
)

# MCP bootstrap.  PhenoMCPServers convention: `from fastmcp import FastMCP`.
try:
    from fastmcp import FastMCP
except ImportError as e:  # pragma: no cover
    sys.stderr.write(
        "forge3_bridge requires the `fastmcp` package. Install with:\n"
        "  pip install 'fastmcp>=3.4.2'\n"
        f"Original error: {e}\n"
    )
    raise


mcp = FastMCP("forge3-bridge")

_TRANSPORT: Optional[Transport] = None


def _transport() -> Transport:
    """Lazy singleton; reconnects on transport failure."""
    global _TRANSPORT
    if _TRANSPORT is None:
        _TRANSPORT = pick_transport(_FakeArgs())
    return _TRANSPORT


class _FakeArgs:
    transport = "auto"
    ws = DEFAULT_WS
    bin = DEFAULT_BIN


def _call(method: str, params: Any = None) -> Any:
    """Single round-trip; resets the singleton on failure so the next call reconnects."""
    global _TRANSPORT
    try:
        return flatten_result(_transport().call(method, params))
    except (TransportError, ForgeRpcError):
        _TRANSPORT = None
        raise


# ----------------------------- tools -----------------------------


@mcp.tool()
def forge3_info() -> dict:
    """Forge3 rpc.info — full extension registry with enable state, version, capabilities."""
    return _call("info")


@mcp.tool()
def forge3_methods() -> list[dict]:
    """Forge3 rpc.discover — list of every JSON-RPC method forge3 accepts, with parameter names."""
    disc = _call("rpc.discover")
    methods = (disc or {}).get("methods", []) if isinstance(disc, dict) else []
    return [
        {"name": m.get("name"), "params": [p.get("name") for p in m.get("params", [])]}
        for m in methods
    ]


@mcp.tool()
def forge3_tools() -> list[dict]:
    """Forge3 tool_list — the LLM-facing tools (read/write/patch/search/shell/task/...)."""
    payload = _call("tool_list") or {}
    tools = payload.get("tools", []) if isinstance(payload, dict) else []
    return [{"name": t.get("name"), "description": (t.get("description") or "")[:240]} for t in tools]


@mcp.tool()
def forge3_extensions() -> list[dict]:
    """Forge3 info.extensions — every extension forge3 has loaded (providers, tools, mcp, ...)."""
    info = _call("info") or {}
    ext = info.get("extensions", {}) if isinstance(info, dict) else {}
    return [
        {
            "id": k,
            "enabled": v.get("enabled"),
            "caps": (v.get("data") or {}).get("capabilities", []),
            "name": (v.get("data") or {}).get("name"),
        }
        for k, v in ext.items()
    ]


@mcp.tool()
def forge3_models() -> list[str]:
    """Forge3 model_list — every model forge3 currently knows about."""
    payload = _call("model_list") or {}
    models = payload.get("models", []) if isinstance(payload, dict) else []
    return [(m.get("name") or m.get("id")) for m in models]


@mcp.tool()
def forge3_agents() -> list[dict]:
    """Forge3 agent_list — the headless agent profiles forge3 exposes (forge/muse/sage)."""
    payload = _call("agent_list") or {}
    agents = payload.get("agents", []) if isinstance(payload, dict) else []
    return [{"id": a.get("id"), "description": (a.get("description") or "")[:240]} for a in agents]


@mcp.tool()
def forge3_commands() -> list[dict]:
    """Forge3 command_list — slash-style commands (auth flows, reload MCP, reload skills, ...)."""
    payload = _call("command_list") or {}
    cmds = payload.get("commands", []) if isinstance(payload, dict) else []
    return [{"name": c.get("name"), "description": (c.get("description") or "")[:200]} for c in cmds]


@mcp.tool()
def forge3_call(method: str, params_json: Optional[str] = None) -> dict:
    """Raw JSON-RPC passthrough. Pass any forge3 method; pass `params_json` as a JSON-encoded object string.

    Example:
        forge3_call(method="tool_call", params_json='{"name":"shell","arguments":{"command":"uname -a"}}')
    """
    params = None
    if params_json:
        try:
            params = json.loads(params_json)
        except json.JSONDecodeError as e:
            return {"error": f"params_json is not valid JSON: {e}"}
    return _call(method, params)


@mcp.tool()
def forge3_shell(command: str, cwd: Optional[str] = None) -> dict:
    """Run a shell command on the forge3 host via tool.shell. Returns stdout/stderr/exit."""
    return _call(
        "tool_call",
        {"name": "shell", "arguments": {"command": command, "cwd": cwd or os.getcwd()}},
    )


@mcp.tool()
def forge3_search(pattern: str, path: Optional[str] = None, mode: str = "files_with_matches") -> dict:
    """Ripgrep-style code search via tool.search. `mode` is one of content/files_with_matches/count."""
    return _call(
        "tool_call",
        {
            "name": "search",
            "arguments": {
                "pattern": pattern,
                "path": path or os.getcwd(),
                "output_mode": mode,
            },
        },
    )


@mcp.tool()
def forge3_read(path: str, max_lines: Optional[int] = None) -> dict:
    """Read a text file via tool.read. `max_lines` truncates if set."""
    args: dict = {"path": path}
    if max_lines:
        args["max_lines"] = int(max_lines)
    return _call("tool_call", {"name": "read", "arguments": args})


@mcp.tool()
def forge3_write(path: str, content: str) -> dict:
    """Write `content` to `path` via tool.write. Overwrites if existing."""
    return _call("tool_call", {"name": "write", "arguments": {"path": path, "content": content}})


@mcp.tool()
def forge3_patch(path: str, old_string: str, new_string: str, replace_all: bool = False) -> dict:
    """Patch a file by exact-string replacement via tool.patch."""
    return _call(
        "tool_call",
        {
            "name": "patch",
            "arguments": {
                "path": path,
                "old_string": old_string,
                "new_string": new_string,
                "replace_all": replace_all,
            },
        },
    )


@mcp.tool()
def forge3_skill_search(query: str) -> dict:
    """Discover available skills by keyword via tool.skill_search."""
    return _call("tool_call", {"name": "skill_search", "arguments": {"query": query}})


@mcp.tool()
def forge3_doctor() -> dict:
    """Probe ws://127.0.0.1:9753 + stdio transport. Useful before relying on the server."""
    ws_reachable, ws_err = _probe_ws()
    stdio_reachable, stdio_err = _probe_stdio()
    return {
        "binary": DEFAULT_BIN,
        "binary_exists": os.path.exists(DEFAULT_BIN),
        "ws_url": DEFAULT_WS,
        "ws_reachable": ws_reachable,
        "ws_error": ws_err,
        "stdio_reachable": stdio_reachable,
        "stdio_error": stdio_err,
        "recommendation": "ws" if ws_reachable else ("stdio" if stdio_reachable else "neither"),
    }


def _probe_ws() -> tuple[bool, Optional[str]]:
    try:
        import asyncio
        try:
            import websockets  # noqa: F401
        except ImportError:
            return False, "websockets not installed"

        async def _open():
            return await websockets.connect(DEFAULT_WS, open_timeout=2, close_timeout=1)

        asyncio.run(_open())
        return True, None
    except Exception as e:
        return False, f"{type(e).__name__}: {e}"


def _probe_stdio() -> tuple[bool, Optional[str]]:
    try:
        StdioTransport(DEFAULT_BIN).call("info", timeout=5)
        return True, None
    except Exception as e:
        return False, f"{type(e).__name__}: {e}"


# ----------------------------- entrypoint -----------------------------


def main() -> None:
    # mcp.run() drives stdio jsonrpc for the MCP protocol.
    mcp.run()


if __name__ == "__main__":
    main()

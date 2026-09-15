#!/usr/bin/env python3
"""
forge3-cli — Python CLI wrapper for the Forge Agent SDK (forge3).

Forge3 is the executable name of the new Forge Agent SDK (v0.1.0). It exposes
a JSON-RPC 2.0 surface over two transports:

    * stdio  —  spawn `forge3 stdio` per call, send one JSON-RPC envelope, read reply
    * ws     —  talk to a long-running `forge3 ws --addr 127.0.0.1:9753` daemon

This CLI unifies both, defaults to ws, and falls back to stdio if no daemon.

Usage examples (see also SKILL.md and `forge3-cli --help`):

    forge3-cli info
    forge3-cli methods                # RPC method list
    forge3-cli tools                  # tool list
    forge3-cli extensions             # extension list
    forge3-cli models                 # model list
    forge3-cli call tool_list
    forge3-cli call tool_call '<json-params>'   # raw JSON-RPC call
    forge3-cli shell 'uname -a'       # high-level: forge3 tool.shell wrapper
    forge3-cli search 'TODO'          # high-level: forge3 tool.search wrapper
    forge3-cli doctor                 # transport + version sanity check

Exit codes:
    0  success
    1  protocol / parse error
    2  forge3 binary missing / not executable
    3  forge3 daemon unreachable (and stdio fallback failed)
    4  JSON-RPC error returned (method-not-found, etc.)
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
import uuid
from dataclasses import dataclass
from typing import Any, Optional

DEFAULT_WS = os.environ.get("FORGE3_WS", "ws://127.0.0.1:9753")
DEFAULT_BIN = os.environ.get("FORGE3_BIN", shutil.which("forge3") or "/Users/kooshapari/.cargo/bin/forge3")


# ----------------------------- transport layer -----------------------------


class TransportError(RuntimeError):
    pass


class ForgeRpcError(RuntimeError):
    def __init__(self, code: int, message: str, data: Any = None):
        super().__init__(f"rpc error {code}: {message}")
        self.code = code
        self.message = message
        self.data = data


@dataclass
class Transport:
    """Pluggable transport. Subclass and override .call() ."""

    def call(self, method: str, params: Any = None, timeout: float = 30.0) -> Any:
        raise NotImplementedError

    def close(self) -> None:
        pass


class WsTransport(Transport):
    """Long-lived WebSocket connection to forge3 ws daemon."""

    def __init__(self, url: str = DEFAULT_WS):
        try:
            import websockets  # noqa: F401
        except ImportError as e:
            raise TransportError(
                "Python `websockets` package missing. Install with:\n"
                "  uv pip install --system websockets\n"
                "or set FORGE3_TRANSPORT=stdio to use stdio fallback."
            ) from e
        self.url = url
        self._ws = None  # lazy

    def _connect(self):
        import asyncio
        import websockets

        async def _open():
            return await websockets.connect(self.url, open_timeout=2, close_timeout=1)

        try:
            return asyncio.run(_open())
        except Exception as e:
            raise TransportError(f"connect {self.url} failed: {e}") from e

    def call(self, method: str, params: Any = None, timeout: float = 30.0) -> Any:
        import asyncio

        if self._ws is None:
            self._ws = self._connect()

        msg_id = str(uuid.uuid4())
        envelope: dict = {"jsonrpc": "2.0", "id": msg_id, "method": method}
        if params is not None:
            envelope["params"] = params

        async def _roundtrip():
            await self._ws.send(json.dumps(envelope))
            while True:
                raw = await asyncio.wait_for(self._ws.recv(), timeout=timeout)
                data = json.loads(raw)
                # Server may push server->client notifications interleaved.
                # Match by id; skip notifications/responses without our id.
                if data.get("id") == msg_id:
                    return data
                # else: notification, keep reading

        try:
            data = asyncio.run(_roundtrip())
        except Exception as e:
            # Drop broken socket so next call reconnects.
            try:
                if self._ws is not None:
                    asyncio.run(self._ws.close())
            except Exception:
                pass
            self._ws = None
            raise TransportError(f"ws call failed: {e}") from e

        if "error" in data:
            err = data["error"]
            raise ForgeRpcError(err.get("code", -32000), err.get("message", ""), err.get("data"))
        return data.get("result")

    def close(self) -> None:
        if self._ws is None:
            return
        import asyncio

        try:
            asyncio.run(self._ws.close())
        except Exception:
            pass
        self._ws = None


class StdioTransport(Transport):
    """Spawn `forge3 stdio` per call. Slower but always works."""

    def __init__(self, bin_path: str = DEFAULT_BIN):
        self.bin = bin_path

    def call(self, method: str, params: Any = None, timeout: float = 30.0) -> Any:
        if not os.path.exists(self.bin):
            raise TransportError(f"forge3 binary not found at {self.bin}")
        envelope: dict = {"jsonrpc": "2.0", "id": str(uuid.uuid4()), "method": method}
        if params is not None:
            envelope["params"] = params
        try:
            proc = subprocess.run(
                [self.bin, "stdio"],
                input=json.dumps(envelope) + "\n",
                capture_output=True,
                text=True,
                timeout=timeout,
            )
        except subprocess.TimeoutExpired as e:
            raise TransportError(f"stdio timeout after {timeout}s") from e
        except FileNotFoundError as e:
            raise TransportError(f"binary not executable: {self.bin}") from e

        if proc.returncode != 0 and not proc.stdout.strip():
            raise TransportError(f"forge3 exit={proc.returncode} stderr={proc.stderr.strip()[:400]}")

        # Read first non-empty line as JSON reply.
        reply: Optional[dict] = None
        for line in proc.stdout.splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                reply = json.loads(line)
                break
            except json.JSONDecodeError:
                continue
        if reply is None:
            raise TransportError(
                f"no json-rpc reply on stdout. raw stdout={proc.stdout[:300]!r} stderr={proc.stderr[:300]!r}"
            )
        if "error" in reply:
            err = reply["error"]
            raise ForgeRpcError(err.get("code", -32000), err.get("message", ""), err.get("data"))
        return reply.get("result")


# ----------------------------- result flattening -----------------------------


def flatten_result(data: Any) -> Any:
    """`rpc.discover`, `tool_list`, etc. wrap their payload in `result.data.complete.X`."""
    if not isinstance(data, dict):
        return data
    if "data" in data and isinstance(data["data"], dict) and "complete" in data["data"]:
        comp = data["data"]["complete"]
        if isinstance(comp, dict) and len(comp) == 1:
            return next(iter(comp.values()))
    return data


def render(obj: Any) -> str:
    return json.dumps(obj, indent=2, ensure_ascii=False)


# ----------------------------- CLI -----------------------------


def cmd_info(args, t: Transport):
    return flatten_result(t.call("info"))


def cmd_methods(args, t: Transport):
    disc = flatten_result(t.call("rpc.discover"))
    methods = disc.get("methods", []) if isinstance(disc, dict) else []
    return [{"name": m.get("name"), "params": [p.get("name") for p in m.get("params", [])]} for m in methods]


def cmd_tools(args, t: Transport):
    payload = flatten_result(t.call("tool_list"))
    tools = payload.get("tools", []) if isinstance(payload, dict) else []
    return [{"name": t.get("name"), "description": (t.get("description") or "")[:120]} for t in tools]


def cmd_extensions(args, t: Transport):
    info = flatten_result(t.call("info"))
    ext = info.get("extensions", {}) if isinstance(info, dict) else {}
    return [
        {"id": k, "enabled": v.get("enabled"), "caps": (v.get("data") or {}).get("capabilities", [])}
        for k, v in ext.items()
    ]


def cmd_models(args, t: Transport):
    payload = flatten_result(t.call("model_list"))
    models = payload.get("models", []) if isinstance(payload, dict) else []
    return [m.get("name") or m.get("id") for m in models]


def cmd_agents(args, t: Transport):
    payload = flatten_result(t.call("agent_list"))
    agents = payload.get("agents", []) if isinstance(payload, dict) else []
    return [{"id": a.get("id"), "description": (a.get("description") or "")[:160]} for a in agents]


def cmd_commands(args, t: Transport):
    payload = flatten_result(t.call("command_list"))
    cmds = payload.get("commands", []) if isinstance(payload, dict) else []
    return [{"name": c.get("name"), "description": (c.get("description") or "")[:120]} for c in cmds]


def cmd_call(args, t: Transport):
    params = None
    if args.params:
        try:
            params = json.loads(args.params)
        except json.JSONDecodeError as e:
            print(f"error: --params must be valid JSON: {e}", file=sys.stderr)
            sys.exit(1)
    return t.call(args.method, params)


def cmd_shell(args, t: Transport):
    """Convenience: invoke the forge3 `tool.shell` extension via the generic tool_call RPC."""
    return t.call(
        "tool_call",
        {"name": "shell", "arguments": {"command": args.command, "cwd": args.cwd or os.getcwd()}},
    )


def cmd_search(args, t: Transport):
    return t.call(
        "tool_call",
        {
            "name": "search",
            "arguments": {
                "pattern": args.pattern,
                "path": args.path or os.getcwd(),
                "output_mode": args.mode,
            },
        },
    )


def cmd_doctor(args, t: Transport):
    out: dict = {"binary": DEFAULT_BIN, "binary_exists": os.path.exists(DEFAULT_BIN)}
    if out["binary_exists"]:
        try:
            # `forge3` with no args would launch stdio server and hang.
            # Use `forge3 help` which exits after printing help text.
            v = subprocess.run([DEFAULT_BIN, "help"], capture_output=True, text=True, timeout=5)
            combined = (v.stdout or "") + (v.stderr or "")
            first = next((ln.strip() for ln in combined.splitlines() if ln.strip()), "")
            out["version"] = first[:120] if first else f"exit={v.returncode}"
            out["version_exit"] = v.returncode
        except Exception as e:
            out["version_error"] = str(e)
    out["ws_url"] = DEFAULT_WS
    ws = WsTransport(DEFAULT_WS)
    try:
        out["ws_reachable"] = True
        info = flatten_result(ws.call("info", timeout=3))
        out["ws_info_keys"] = sorted((info or {}).keys()) if isinstance(info, dict) else type(info).__name__
    except Exception as e:
        out["ws_reachable"] = False
        out["ws_error"] = str(e)
    finally:
        ws.close()

    stdio = StdioTransport(DEFAULT_BIN)
    try:
        out["stdio_reachable"] = True
        info = flatten_result(stdio.call("info", timeout=5))
        out["stdio_info_keys"] = sorted((info or {}).keys()) if isinstance(info, dict) else type(info).__name__
    except Exception as e:
        out["stdio_reachable"] = False
        out["stdio_error"] = str(e)

    out["recommendation"] = (
        "ws" if out.get("ws_reachable") else ("stdio" if out.get("stdio_reachable") else "neither")
    )
    return out


# ----------------------------- transport selection -----------------------------


def pick_transport(args) -> Transport:
    choice = (args.transport or os.environ.get("FORGE3_TRANSPORT") or "auto").lower()
    if choice == "ws":
        return WsTransport(args.ws or DEFAULT_WS)
    if choice == "stdio":
        return StdioTransport(args.bin or DEFAULT_BIN)
    # auto: prefer ws, fall back to stdio
    ws = WsTransport(args.ws or DEFAULT_WS)
    try:
        ws.call("info", timeout=2)
        return ws
    except Exception:
        ws.close()
    return StdioTransport(args.bin or DEFAULT_BIN)


def main(argv: Optional[list] = None) -> int:
    p = argparse.ArgumentParser(
        prog="forge3-cli",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--transport", choices=["auto", "ws", "stdio"], default="auto")
    p.add_argument("--ws", default=DEFAULT_WS, help=f"WebSocket URL (default: {DEFAULT_WS})")
    p.add_argument("--bin", default=DEFAULT_BIN, help=f"forge3 binary path (default: {DEFAULT_BIN})")
    p.add_argument("--quiet", action="store_true", help="suppress non-essential stderr")

    sub = p.add_subparsers(dest="cmd", required=True)

    sub.add_parser("info", help="rpc.info (extension registry + enabled state)")
    sub.add_parser("methods", help="rpc.discover method list")
    sub.add_parser("tools", help="tool_list (for LLM tool-use)")
    sub.add_parser("extensions", help="info.extensions — full extension registry")
    sub.add_parser("models", help="model_list — all models forge3 knows about")
    sub.add_parser("agents", help="agent_list — forge/muse/sage agent profiles")
    sub.add_parser("commands", help="command_list — slash-command style ops")
    sub.add_parser("doctor", help="probe ws + stdio transports and print verdict")

    pc = sub.add_parser("call", help="raw json-rpc call: forge3-cli call <method> [--params '<json>']")
    pc.add_argument("method")
    pc.add_argument("--params", help="JSON-encoded params object (omit for unit methods)")

    ps = sub.add_parser("shell", help="high-level: run a shell command via tool.shell")
    ps.add_argument("command")
    ps.add_argument("--cwd")

    psg = sub.add_parser("search", help="high-level: ripgrep-style search via tool.search")
    psg.add_argument("pattern")
    psg.add_argument("--path", help="search root (default: cwd)")
    psg.add_argument("--mode", default="files_with_matches", choices=["content", "files_with_matches", "count"])

    args = p.parse_args(argv)

    t0 = time.time()
    try:
        t = pick_transport(args)
    except TransportError as e:
        print(f"error: {e}", file=sys.stderr)
        return 3

    handlers = {
        "info": cmd_info,
        "methods": cmd_methods,
        "tools": cmd_tools,
        "extensions": cmd_extensions,
        "models": cmd_models,
        "agents": cmd_agents,
        "commands": cmd_commands,
        "call": cmd_call,
        "shell": cmd_shell,
        "search": cmd_search,
        "doctor": cmd_doctor,
    }
    try:
        result = handlers[args.cmd](args, t)
    except ForgeRpcError as e:
        print(f"error: {e}", file=sys.stderr)
        return 4
    except TransportError as e:
        print(f"transport error: {e}", file=sys.stderr)
        return 3
    finally:
        t.close()

    print(render(result))
    if not args.quiet:
        elapsed_ms = int((time.time() - t0) * 1000)
        print(f"# forge3-cli {args.cmd} OK in {elapsed_ms} ms", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
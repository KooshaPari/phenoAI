"""pytest: forge3-bridge MCP server smoke tests.

Verifies the server responds to MCP initialize + tools/list and can execute one
real tool call (forge3_doctor) against the local forge3 binary.
"""

from __future__ import annotations

import json
import os
import queue
import select
import subprocess
import sys
import threading
import time
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent
SERVER = REPO / "forge3_bridge_server.py"
LEGACY_SERVER = REPO / "forge3_mcp.py"


def _run_mcp(
    messages: list[dict], timeout: int = 30, server: Path = SERVER
) -> list[dict]:
    """Spawn the MCP server and exchange each JSON-RPC message in order."""
    proc = subprocess.Popen(
        [sys.executable, str(server)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        cwd=str(REPO),
    )
    lines: queue.Queue = queue.Queue()

    def read_stdout() -> None:
        assert proc.stdout is not None
        for line in proc.stdout:
            lines.put(line)
        lines.put(None)

    reader = threading.Thread(target=read_stdout, daemon=True)
    reader.start()
    replies = []
    try:
        for message in messages:
            assert proc.stdin is not None
            assert proc.stdout is not None
            proc.stdin.write(json.dumps(message) + "\n")
            proc.stdin.flush()
            if "id" not in message:
                continue
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                remaining = deadline - time.monotonic()
                try:
                    line = lines.get(timeout=min(remaining, 0.1))
                except queue.Empty:
                    continue
                if line is None:
                    break
                line = line.strip()
                if not line or not line.startswith("{"):
                    continue
                try:
                    reply = json.loads(line)
                except json.JSONDecodeError:
                    continue
                replies.append(reply)
                if reply.get("id") == message["id"]:
                    break
            else:
                raise AssertionError(f"no JSON-RPC reply for {message['id']}")
    finally:
        if proc.stdin is not None:
            proc.stdin.close()
        proc.terminate()
        try:
            proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=timeout)
        reader.join(timeout=timeout)
    return replies


def test_run_mcp_handles_noisy_stderr_and_forces_uncooperative_shutdown(
    tmp_path: Path,
):
    """A protocol reply survives stderr noise and a child that ignores SIGTERM."""
    server = tmp_path / "noisy_server.py"
    server.write_text(
        """import json
import signal
import sys

signal.signal(signal.SIGTERM, signal.SIG_IGN)
sys.stderr.write("x" * 131072 + "\\n")
sys.stderr.flush()
for line in sys.stdin:
    request = json.loads(line)
    if "id" in request:
        print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": {}}), flush=True)
"""
    )

    replies = _run_mcp(
        [{"jsonrpc": "2.0", "id": "noisy", "method": "initialize"}],
        timeout=1,
        server=server,
    )

    assert replies == [{"jsonrpc": "2.0", "id": "noisy", "result": {}}]


def test_run_mcp_uses_portable_pipe_reading(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    """Anonymous-pipe handling does not depend on select(), which Windows rejects."""
    server = tmp_path / "replying_server.py"
    server.write_text(
        """import json
import sys

for line in sys.stdin:
    request = json.loads(line)
    if "id" in request:
        print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": {}}), flush=True)
"""
    )

    def unsupported_select(*_args: object, **_kwargs: object) -> object:
        raise OSError("anonymous pipes are unsupported")

    monkeypatch.setattr(select, "select", unsupported_select)
    replies = _run_mcp(
        [{"jsonrpc": "2.0", "id": "portable", "method": "initialize"}],
        timeout=1,
        server=server,
    )

    assert replies == [{"jsonrpc": "2.0", "id": "portable", "result": {}}]


def test_legacy_entrypoint_exposes_the_canonical_mcp_surface():
    """Legacy filename remains a thin, protocol-compatible entrypoint."""
    assert LEGACY_SERVER.is_file(), "missing legacy forge3_mcp.py entrypoint"
    replies = _run_mcp(
        [
            {
                "jsonrpc": "2.0",
                "id": "legacy-init",
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "pytest", "version": "0.0.1"},
                },
            },
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": "legacy-tools", "method": "tools/list"},
        ],
        server=LEGACY_SERVER,
    )
    replies_by_id = {reply.get("id"): reply for reply in replies}
    assert (
        replies_by_id["legacy-init"]["result"]["serverInfo"]["name"] == "forge3-bridge"
    )
    assert "forge3_doctor" in {
        tool["name"] for tool in replies_by_id["legacy-tools"]["result"]["tools"]
    }


def test_server_responds_to_initialize_and_tools_list():
    """Server returns initialize result + a tools/list with all 15 tools."""
    replies = _run_mcp(
        [
            {
                "jsonrpc": "2.0",
                "id": "t1",
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "pytest", "version": "0.0.1"},
                },
            },
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": "t2", "method": "tools/list"},
        ]
    )
    assert len(replies) == 2, f"expected 2 JSON-RPC replies, got {len(replies)}"

    init = replies[0]
    assert init["id"] == "t1"
    assert "result" in init
    assert init["result"]["serverInfo"]["name"] == "forge3-bridge"

    tools = replies[1]
    assert tools["id"] == "t2"
    tool_names = [t["name"] for t in tools["result"]["tools"]]
    expected = {
        "forge3_info",
        "forge3_doctor",
        "forge3_methods",
        "forge3_tools",
        "forge3_extensions",
        "forge3_models",
        "forge3_agents",
        "forge3_commands",
        "forge3_call",
        "forge3_shell",
        "forge3_search",
        "forge3_read",
        "forge3_write",
        "forge3_patch",
        "forge3_skill_search",
    }
    missing = expected - set(tool_names)
    assert not missing, f"missing tools: {missing}"
    assert len(tool_names) >= 15


@pytest.mark.skipif(
    not os.path.exists(
        os.environ.get("FORGE3_BIN", "/Users/kooshapari/.cargo/bin/forge3")
    ),
    reason="forge3 binary not available",
)
def test_doctor_tool_call_against_real_daemon():
    """Real forge3_doctor call returns a verdict dict."""
    replies = _run_mcp(
        [
            {
                "jsonrpc": "2.0",
                "id": "d1",
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "pytest", "version": "0.0.1"},
                },
            },
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {
                "jsonrpc": "2.0",
                "id": "d2",
                "method": "tools/call",
                "params": {"name": "forge3_doctor", "arguments": {}},
            },
        ]
    )
    call_reply = replies[-1]
    assert call_reply["id"] == "d2"
    assert "result" in call_reply
    result = call_reply["result"]
    # FastMCP 2.x envelope: prefer structuredContent (already a dict), fall
    # back to parsing the first TextContent item's text payload.
    if isinstance(result, dict) and "structuredContent" in result:
        parsed = result["structuredContent"]
    else:
        content = result.get("content", [])
        text = (
            content[0]["text"]
            if isinstance(content, list) and content
            else json.dumps(result)
        )
        parsed = json.loads(text)
    assert "binary" in parsed
    assert "recommendation" in parsed

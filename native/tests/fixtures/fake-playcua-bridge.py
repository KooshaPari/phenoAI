#!/usr/bin/env python3
"""fake-playcua-bridge.py — hermetic NDJSON JSON-RPC peer for sandbox I/O tests.

Speaks the same newline-delimited JSON-RPC 2.0 surface as playcua-native
(screenshot / input.* / windows.*). Used when PLAYCUA_BRIDGE_BIN points here.
"""
from __future__ import annotations

import json
import sys


def respond(req_id, result=None, error=None):
    msg = {"jsonrpc": "2.0", "id": req_id}
    if error is not None:
        msg["error"] = error
    else:
        msg["result"] = result
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


def main() -> int:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError as e:
            respond(None, error={"code": -32700, "message": f"parse error: {e}"})
            continue
        req_id = req.get("id")
        method = req.get("method", "")
        params = req.get("params") or {}

        if method == "ping":
            respond(req_id, {"ok": True, "fake": True})
        elif method == "screenshot":
            respond(
                req_id,
                {
                    "data": "ZmFrZS1wbmc=",
                    "width": 8,
                    "height": 4,
                    "format": "png",
                    "window_title": params.get("window_title"),
                    "monitor": params.get("monitor", 0),
                },
            )
        elif method in ("input.key", "input.type", "input.click", "input.scroll", "input.move"):
            respond(req_id, {"ok": True, "method": method, "params": params})
        elif method == "windows.list":
            respond(
                req_id,
                [
                    {
                        "hwnd": 1,
                        "title": "FakeSandboxWindow",
                        "pid": 99,
                        "x": 0,
                        "y": 0,
                        "width": 640,
                        "height": 480,
                        "visible": True,
                    }
                ],
            )
        elif method == "windows.find":
            title = (params.get("title") or "").lower()
            if "fake" in title or not title:
                respond(
                    req_id,
                    {
                        "hwnd": 1,
                        "title": "FakeSandboxWindow",
                        "pid": 99,
                        "x": 0,
                        "y": 0,
                        "width": 640,
                        "height": 480,
                        "visible": True,
                    },
                )
            else:
                respond(req_id, None)
        elif method == "windows.focus":
            respond(req_id, {"ok": True, "hwnd": params.get("hwnd")})
        else:
            respond(
                req_id,
                error={"code": -32601, "message": f"Method not found: {method}"},
            )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

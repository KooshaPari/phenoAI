"""Smoke test: render the mcp-server-ts7 stub template."""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
RENDERER = ROOT / "templates" / "mcp-server-ts7" / "scripts" / "render.py"


@pytest.fixture()
def rendered_dir(tmp_path: Path) -> Path:
    out = tmp_path / "out"
    cmd = [
        sys.executable,
        str(RENDERER),
        "--id",
        "echo-ts7",
        "--title",
        "Echo MCP TS7",
        "--description",
        "Echoes input back (stub)",
        "--env",
        "ECHO_TOKEN",
        "--out",
        str(out),
    ]
    result = subprocess.run(cmd, capture_output=True, text=True)
    assert result.returncode == 0, f"renderer failed:\n{result.stdout}\n{result.stderr}"
    return out


def test_ts7_server_ts_present(rendered_dir: Path) -> None:
    index_ts = rendered_dir / "servers" / "echo-ts7" / "src" / "index.ts"
    assert index_ts.is_file()
    body = index_ts.read_text(encoding="utf-8")
    assert "echo-ts7" in body


def test_ts7_package_json_present(rendered_dir: Path) -> None:
    pkg = rendered_dir / "servers" / "echo-ts7" / "package.json"
    assert pkg.is_file()
    assert "@modelcontextprotocol/server" in pkg.read_text(encoding="utf-8")


def test_ts7_catalog_entry_present(rendered_dir: Path) -> None:
    entry = rendered_dir / "catalog_entry.rendered.yaml"
    assert entry.is_file()
    assert "id: echo-ts7" in entry.read_text(encoding="utf-8")

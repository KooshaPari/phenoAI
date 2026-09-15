"""eco-026: phenotype-bundle mcp.json wiring matches catalog."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_validate_bundle_wiring_passes() -> None:
    proc = subprocess.run(
        [sys.executable, "scripts/validate_bundle_wiring.py"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    assert proc.returncode == 0, proc.stdout + proc.stderr

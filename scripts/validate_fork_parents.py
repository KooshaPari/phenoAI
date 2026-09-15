#!/usr/bin/env python3
"""Validate framework fork_parent fields per ADR-017."""
from __future__ import annotations

import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]

NORMATIVE = {
    "rust": "Dicklesworthstone/fastmcp_rust",
    "rmcp": "modelcontextprotocol/rust-sdk",
    "go": "mark3labs/mcp-go",
    "python": "PrefectHQ/fastmcp",
}

FORBIDDEN = {
    "rust": {"modelcontextprotocol/rust-sdk"},
}


def main() -> int:
    catalog = yaml.safe_load((ROOT / "catalog" / "registry.yaml").read_text(encoding="utf-8"))
    framework = catalog.get("framework", {})
    errors: list[str] = []

    for lane, expected_parent in NORMATIVE.items():
        entry = framework.get(lane)
        if not entry:
            errors.append(f"missing framework.{lane}")
            continue
        parent = entry.get("fork_parent", "")
        if not parent:
            errors.append(f"framework.{lane} missing fork_parent")
        elif parent != expected_parent:
            errors.append(f"framework.{lane} fork_parent={parent!r} want {expected_parent!r}")
        if parent in FORBIDDEN.get(lane, set()):
            errors.append(f"framework.{lane} uses forbidden parent {parent!r}")

    retired = catalog.get("retired_anti_patterns", [])
    for item in retired:
        if item.get("id") == "phenotype-rust-sdk":
            repl = " ".join(str(x) for x in item.get("replacement", []))
            if "rmcp)" in repl and "PhenoRMCP" not in repl:
                errors.append("phenotype-rust-sdk replacement still conflates rmcp with PhenoFastMCP-rust")

    if errors:
        for e in errors:
            print(f"INVALID: {e}")
        return 1

    print(f"OK fork_parents lanes={sorted(NORMATIVE)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

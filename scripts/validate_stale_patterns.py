#!/usr/bin/env python3
"""Fail on banned MCP anti-patterns in catalog and docs (ADR-017/018)."""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# (pattern, human label) — scan catalog + docs only (not full git history)
BANNED = [
    (r"(?<!never )mirror-to-empty", "mirror-to-empty fork bootstrap"),
    (r"cheap-llm-mcp", "standalone cheap-llm MCP repo"),
    (r"PhenoFastMCP-rust\s+# tier-0 MCP framework fork \(rmcp\)", "rmcp mislabeled as PhenoFastMCP-rust parent"),
    (r"phenotype-rust-sdk.*successor_role:\s*mcp-framework", "retired rust bucket as framework home"),
]

SCAN_GLOBS = [
    "catalog/*.yaml",
    "docs/**/*.md",
    "README.md",
    "skills/**/*.md",
]


def main() -> int:
    hits: list[str] = []
    for pattern in SCAN_GLOBS:
        for path in ROOT.glob(pattern):
            if not path.is_file():
                continue
            text = path.read_text(encoding="utf-8", errors="replace")
            rel = path.relative_to(ROOT).as_posix()
            for regex, label in BANNED:
                if re.search(regex, text, re.IGNORECASE):
                    hits.append(f"{rel}: banned pattern ({label})")

    if hits:
        for h in hits:
            print(f"STALE: {h}")
        return 1

    print("OK no banned MCP patterns in catalog/docs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

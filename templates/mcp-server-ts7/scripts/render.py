#!/usr/bin/env python3
"""Render the mcp-server-ts7 template stub to an output directory.

PLANNED STUB — no CI test yet (sd-ts7 defer gate G4).

Usage:
    python templates/mcp-server-ts7/scripts/render.py \\
        --id echo --title "Echo MCP" --description "Echoes input back" \\
        --out /tmp/render
"""
from __future__ import annotations

import argparse
import re
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
TEMPLATE_DIR = HERE.parent
PLACEHOLDER = re.compile(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\}\}")


def parse_env(raw: str | None) -> list[str]:
    if not raw:
        return []
    return [item.strip() for item in raw.split(",") if item.strip()]


def render_string(text: str, mapping: dict[str, str]) -> str:
    def repl(match: re.Match[str]) -> str:
        key = match.group(1)
        return mapping.get(key, match.group(0))

    return PLACEHOLDER.sub(repl, text)


def render_file(src: Path, dst: Path, mapping: dict[str, str]) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text(render_string(src.read_text(encoding="utf-8"), mapping), encoding="utf-8")


def build_mapping(args: argparse.Namespace) -> dict[str, str]:
    env = parse_env(args.env)
    env_list_lines = "\n".join(f"      - {name}" for name in env) if env else "      []"
    return {
        "id": args.id,
        "title": args.title or args.id,
        "description": args.description or f"{args.title or args.id} MCP server",
        "transport": args.transport,
        "env": args.env or "",
        "env_list": env_list_lines,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--id", required=True)
    parser.add_argument("--title", default=None)
    parser.add_argument("--description", default=None)
    parser.add_argument("--transport", default="stdio")
    parser.add_argument("--env", default=None, help="comma-separated env var names")
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()

    out_dir = args.out.resolve()
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True)

    mapping = build_mapping(args)
    server_dir = out_dir / "servers" / mapping["id"] / "src"
    render_file(TEMPLATE_DIR / "server.ts.tmpl", server_dir / "index.ts", mapping)
    render_file(TEMPLATE_DIR / "package.json.tmpl", out_dir / "servers" / mapping["id"] / "package.json", mapping)
    catalog_path = out_dir / "catalog_entry.rendered.yaml"
    render_file(TEMPLATE_DIR / "catalog_entry.yaml.tmpl", catalog_path, mapping)

    print(f"wrote {server_dir.relative_to(out_dir)}/index.ts")
    print(f"wrote servers/{mapping['id']}/package.json")
    print(f"wrote {catalog_path.relative_to(out_dir)}")
    print("WARN: mcp-server-ts7 is a PLANNED stub — validate_catalog not run")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

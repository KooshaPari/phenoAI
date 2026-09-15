#!/usr/bin/env python3
"""Render the HexaKit mcp-server template to an output directory.

Usage:
    python templates/mcp-server/scripts/render.py \\
        --id echo --title "Echo MCP" --description "Echoes input back" \\
        --out /tmp/render

The renderer performs simple ``{{var}}`` substitution across the template
files in this directory. It also validates the rendered catalog entry
shape against ``schemas/registry.schema.json`` from the PhenoMCPServers
checkout that owns the output directory.
"""
from __future__ import annotations

import argparse
import json
import re
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
TEMPLATE_DIR = HERE.parent
REPO_ROOT_CANDIDATES = [TEMPLATE_DIR.parent, HERE]

PLACEHOLDER = re.compile(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\}\}")


def find_repo_root(start: Path) -> Path | None:
    for candidate in [start, *start.parents]:
        if (candidate / "catalog" / "registry.yaml").is_file() and (
            candidate / "schemas" / "registry.schema.json"
        ).is_file():
            return candidate
    return None


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
    env_json_lines = (
        ",\n".join(f'        "{name}": ""' for name in env) if env else ""
    )
    return {
        "id": args.id,
        "title": args.title or args.id,
        "description": args.description or f"{args.title or args.id} MCP server",
        "transport": args.transport,
        "framework": args.framework,
        "module": args.module,
        "env": args.env or "",
        "env_list": env_list_lines,
        "env_json": env_json_lines,
        "id_upper": args.id.upper().replace("-", "_"),
    }


def render_template(out_dir: Path, mapping: dict[str, str]) -> dict[str, Path]:
    targets: dict[str, Path] = {}

    server_dir = out_dir / "servers" / mapping["id"]
    skill_dir = out_dir / "skills" / f"{mapping['id']}-skill"
    plugin_dir = out_dir / "plugins" / f"{mapping['id']}-bundle"
    agent_dir = out_dir / "agents" / f"{mapping['id']}-lead"

    pairs = [
        ("server.py.tmpl", server_dir / f"{mapping['module']}.py"),
        ("pyproject.toml.tmpl", server_dir / "pyproject.toml"),
        ("SKILL.md.tmpl", skill_dir / "SKILL.md"),
        ("skill.yaml.tmpl", skill_dir / "skill.yaml"),
        ("plugin.yaml.tmpl", plugin_dir / "plugin.yaml"),
        ("mcp.json.tmpl", plugin_dir / "mcp.json"),
        ("agent.yaml.tmpl", agent_dir / "agent.yaml"),
    ]
    for src_name, dst_path in pairs:
        render_file(TEMPLATE_DIR / src_name, dst_path, mapping)
        targets[src_name] = dst_path

    catalog_path = out_dir / "catalog_entry.rendered.yaml"
    render_file(TEMPLATE_DIR / "catalog_entry.yaml.tmpl", catalog_path, mapping)
    targets["catalog_entry.yaml.tmpl"] = catalog_path
    return targets


def validate_catalog_entry(yaml_path: Path, repo_root: Path) -> None:
    try:
        import yaml
    except ImportError:
        print("pyyaml required for validation", file=sys.stderr)
        return
    try:
        from jsonschema import Draft202012Validator
    except ImportError:
        print("jsonschema required for validation", file=sys.stderr)
        return

    rendered = yaml.safe_load(yaml_path.read_text(encoding="utf-8"))
    schema_path = repo_root / "schemas" / "registry.schema.json"
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    validator = Draft202012Validator(schema)
    errors: list = []
    defs = schema.get("$defs", {})
    for entry in rendered.get("servers", []):
        sub = Draft202012Validator(defs["server"])
        errors.extend(sub.iter_errors(entry))
    for section, def_name in (("skills", "skill"), ("plugins", "plugin"), ("agents", "agent")):
        for entry in rendered.get(section, []):
            sub = Draft202012Validator(defs[def_name])
            errors.extend(sub.iter_errors(entry))
    if errors:
        for err in errors:
            print(f"INVALID: {list(err.path)}: {err.message}", file=sys.stderr)
        sys.exit(1)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--id", required=True)
    parser.add_argument("--title", default=None)
    parser.add_argument("--description", default=None)
    parser.add_argument("--transport", default="stdio")
    parser.add_argument("--framework", default="fastmcp")
    parser.add_argument("--module", default=None)
    parser.add_argument("--env", default=None, help="comma-separated env var names")
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    if args.module is None:
        args.module = f"{args.id.replace('-', '_')}_server"

    out_dir = args.out.resolve()
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True)

    repo_root = find_repo_root(TEMPLATE_DIR) or find_repo_root(out_dir)
    mapping = build_mapping(args)
    rendered = render_template(out_dir, mapping)

    for name, path in rendered.items():
        print(f"wrote {path.relative_to(out_dir)} ({name})")

    if repo_root is not None:
        validate_catalog_entry(
            rendered["catalog_entry.yaml.tmpl"], repo_root
        )
        print(f"OK catalog entry validates against {repo_root}/schemas/registry.schema.json")
    else:
        print("WARN: no PhenoMCPServers repo root found; skipping schema validation")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Validate catalog/registry.yaml against schemas/registry.schema.json."""
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SECTIONS = ("servers", "skills", "plugins", "agents")


def duplicate_ids(catalog: dict) -> list[tuple[str, str]]:
    """Return duplicate entry IDs within each registry namespace."""
    duplicates = []
    for section in SECTIONS:
        seen = set()
        for entry in catalog.get(section, []):
            entry_id = entry.get("id")
            if entry_id in seen:
                duplicates.append((section, entry_id))
            else:
                seen.add(entry_id)
    return duplicates


def main() -> int:
    try:
        import yaml
    except ImportError:
        print("pip install pyyaml jsonschema", file=sys.stderr)
        return 1

    try:
        from jsonschema import Draft202012Validator
    except ImportError:
        print("pip install jsonschema", file=sys.stderr)
        return 1

    catalog_path = ROOT / "catalog" / "registry.yaml"
    schema_path = ROOT / "schemas" / "registry.schema.json"

    catalog = yaml.safe_load(catalog_path.read_text(encoding="utf-8"))
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    validator = Draft202012Validator(schema)
    errors = sorted(validator.iter_errors(catalog), key=lambda e: list(e.path))
    if errors:
        for err in errors:
            print(f"INVALID: {list(err.path)}: {err.message}")
        return 1

    duplicates = duplicate_ids(catalog)
    if duplicates:
        for section, entry_id in duplicates:
            print(f"DUPLICATE ID: {section}/{entry_id}")
        return 1

    for section in SECTIONS:
        for entry in catalog.get(section, []):
            rel = entry.get("path") or entry.get("package")
            if rel and not str(rel).startswith("external"):
                p = ROOT / rel
                if entry.get("status") == "active" and not p.exists():
                    print(f"MISSING PATH: {section}/{entry['id']} -> {rel}")
                    return 1

    print(f"OK registry_version={catalog['registry_version']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

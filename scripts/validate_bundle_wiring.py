#!/usr/bin/env python3
"""Validate plugin bundle mcp.json matches catalog registry server wiring."""
from __future__ import annotations

import json
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
BUNDLE_ID = "phenotype-mcp-bundle"
BUNDLE_DIR = ROOT / "plugins" / "phenotype-bundle"


def main() -> int:
    catalog = yaml.safe_load((ROOT / "catalog" / "registry.yaml").read_text(encoding="utf-8"))
    plugins = {p["id"]: p for p in catalog.get("plugins", [])}
    servers = {s["id"]: s for s in catalog.get("servers", [])}

    plugin = plugins.get(BUNDLE_ID)
    if not plugin:
        print(f"MISSING plugin id={BUNDLE_ID} in catalog")
        return 1

    plugin_yaml = yaml.safe_load((BUNDLE_DIR / "plugin.yaml").read_text(encoding="utf-8"))
    mcp = json.loads((BUNDLE_DIR / "mcp.json").read_text(encoding="utf-8"))
    mcp_servers = set(mcp.get("mcpServers", {}))
    registry_servers = set(plugin.get("servers", []))
    yaml_servers = set(plugin_yaml.get("servers", []))

    if registry_servers != yaml_servers:
        print(f"MISMATCH plugin.yaml vs catalog: {yaml_servers} != {registry_servers}")
        return 1
    if mcp_servers != registry_servers:
        print(f"MISMATCH mcp.json vs catalog: {mcp_servers} != {registry_servers}")
        return 1

    for sid in registry_servers:
        entry = servers.get(sid)
        if not entry:
            print(f"UNKNOWN server id in bundle: {sid}")
            return 1
        pkg = entry.get("package", "")
        entry_mod = entry.get("entry", {}).get("module", "")
        cfg = mcp["mcpServers"][sid]
        args = cfg.get("args", [])
        if not args:
            print(f"EMPTY args for {sid}")
            return 1
        script = Path(args[-1])
        expected = ROOT / pkg / f"{entry_mod}.py"
        if script.as_posix() != (Path(pkg) / f"{entry_mod}.py").as_posix():
            print(f"PATH MISMATCH {sid}: mcp.json args[-1]={script} expected {pkg}/{entry_mod}.py")
            return 1
        if not expected.exists():
            print(f"MISSING entry script: {expected}")
            return 1

    print(f"OK bundle={BUNDLE_ID} servers={sorted(registry_servers)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

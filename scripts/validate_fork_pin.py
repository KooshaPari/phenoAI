#!/usr/bin/env python3
"""Validate that each server claiming `framework: fastmcp` actually pins
the documented hard-fork dependency in its `pyproject.toml` /
`requirements.txt` — not upstream fastmcp from PyPI.

Traces to: audit/2026-09-05 PhenoMCPServers.
  "check dependency pin resolves intended hard fork"

Why this exists
---------------

The catalog (catalog/registry.yaml) declares which framework each server
runs under. For `framework: fastmcp` (the Python binding lane), the
documented hard fork is `KooshaPari/PhenoFastMCP` — see
`framework.python.fork_parent: PrefectHQ/fastmcp` and the
`framework.python.repo: https://github.com/KooshaPari/PhenoFastMCP`.

If a server's `pyproject.toml` or `requirements.txt` pins
`fastmcp>=X.Y.Z` (upstream PyPI), a fresh `pip install -r requirements.txt`
will resolve to upstream fastmcp, not the fork — silently breaking
every catalog guarantee (tool registration shape, transport behavior,
patch set). The fork-intent is currently documented only as a
**comment** in some servers' `pyproject.toml` (e.g. substrate's
"# phenofastmcp @ git+...v3.4.2"). A comment does not constrain pip.

This validator makes the pin explicit. For each active `framework: fastmcp`
server it inspects the server's `pyproject.toml` + `requirements.txt` and
requires one of:

  1. A `phenofastmcp @ git+https://github.com/KooshaPari/PhenoFastMCP@<tag>`
     pin (the hard fork), OR
  2. A `# phenofastmcp-fork-pin: <url-or-reason>` directive in the same
     file that documents the deviation and points at a real fork URL —
     e.g. a vendored mirror, a private index, or a planned upgrade.

A plain `fastmcp>=X.Y.Z` pin (no fork reference) is reported as an
error. This keeps the contract enforceable without forcing a specific
resolution (vendored, git+https, private index, etc.) on every server.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
FORK_URL = "github.com/KooshaPari/PhenoFastMCP"
# The fork package name on PyPI-style indexes; the catalog's binding
# lane refers to the fork as "phenofastmcp" (see framework.python.note
# in registry.yaml and the commented pin in servers/substrate/pyproject.toml).
FORK_PKG_NAMES = {"phenofastmcp"}
UPSTREAM_PKG_NAMES = {"fastmcp"}

# Regex for a PEP 508 / requirements.txt line that mentions a name.
# Captures the package name (lowercase, may include . or _ or -).
# Tolerates:
#   - leading `"` (PEP 508 quoted form in pyproject.toml)
#   - leading whitespace (TOML list indentation)
#   - inline `# ...` comments
# Does NOT match a line that starts with `#` — that's a comment line,
# not a dependency. Comment-line skipping happens at the call site
# (we already filter stripped.startswith("#") before running this).
_PKG_LINE_RE = re.compile(
    r"^\s*[\"']?(?P<name>[a-zA-Z][a-zA-Z0-9_.\-]*)\s*[@<=>!~]?",
)

# A "fork directive" is a comment line that starts with this prefix and
# names a real URL or repo slug. We deliberately accept any URL — the
# validator's job is to make the deviation visible, not to second-guess
# the URL choice.
_FORK_DIRECTIVE_RE = re.compile(
    r"^\s*#\s*phenofastmcp-fork-pin:\s*(?P<reason>.+?)\s*$",
    re.MULTILINE,
)


def _read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8") if path.exists() else ""


def _package_hits(text: str, names: set[str]) -> list[str]:
    """Return every line in `text` that pins one of `names`."""
    hits: list[str] = []
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        m = _PKG_LINE_RE.match(stripped)
        if m and m.group("name").lower() in {n.lower() for n in names}:
            hits.append(stripped)
    return hits


def _has_fork_url(text: str) -> bool:
    """Return True only if the fork URL appears on a non-comment line.

    A commented-out `# phenofastmcp @ git+...` line documents intent
    but does not constrain pip. Requiring the URL on an actual
    dependency line is what makes the audit target enforceable.
    """
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if FORK_URL.lower() in stripped.lower():
            return True
    return False


def _has_fork_directive(text: str) -> bool:
    return bool(_FORK_DIRECTIVE_RE.search(text))


def check_server(server: dict) -> list[str]:
    """Return a list of error strings for one server entry.

    Empty list means the server's fork pin is acceptable.
    """
    framework = (server.get("framework") or "").lower()
    if framework != "fastmcp":
        # Only Python fastmcp binding is in scope; Rust/Go/TS use
        # different framework strings and different fork verification
        # (see validate_fork_parents.py for the catalog-level check).
        return []

    status = (server.get("status") or "").lower()
    if status not in {"active", "template"}:
        # Pointer / deprecated entries are external; their fork
        # resolution lives in the upstream repo, not here.
        return []

    package = server.get("package") or server.get("path")
    if not package:
        return [f"{server.get('id')!r}: no package/path to inspect"]

    server_dir = ROOT / package
    if not server_dir.exists():
        # Already caught by validate_catalog.py; skip here to avoid
        # duplicate noise.
        return []

    pyproject_text = _read_text(server_dir / "pyproject.toml")
    req_text = _read_text(server_dir / "requirements.txt")
    combined = pyproject_text + "\n" + req_text

    fork_pin_hits = _package_hits(combined, FORK_PKG_NAMES)
    fork_url_hits = _has_fork_url(combined)
    fork_directive = _has_fork_directive(combined)
    upstream_pin_hits = _package_hits(combined, UPSTREAM_PKG_NAMES)

    errors: list[str] = []

    if not (fork_pin_hits or fork_url_hits or fork_directive):
        errors.append(
            f"{server.get('id')!r}: framework=fastmcp but no "
            f"hard-fork pin. Expected one of:\n"
            f"  - phenofastmcp @ git+https://{FORK_URL}@<tag>\n"
            f"  - any dep mentioning https://{FORK_URL}\n"
            f"  - '# phenofastmcp-fork-pin: <reason>' comment documenting deviation\n"
            f"  Currently pins upstream: {upstream_pin_hits or '(none)'}",
        )

    return errors


def main() -> int:
    catalog = yaml.safe_load(
        (ROOT / "catalog" / "registry.yaml").read_text(encoding="utf-8")
    )
    servers = catalog.get("servers") or []
    errors: list[str] = []
    for server in servers:
        errors.extend(check_server(server))

    if errors:
        print("INVALID fork pin(s):", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    # Count what we validated for the OK line — useful in CI logs.
    checked = sum(
        1
        for s in servers
        if (s.get("framework") or "").lower() == "fastmcp"
        and (s.get("status") or "").lower() in {"active", "template"}
    )
    print(f"OK fork-pin checked={checked} registry_version={catalog.get('registry_version')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

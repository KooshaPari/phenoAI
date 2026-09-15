"""Unit tests for `scripts/validate_fork_pin.py`.

These lock the validator's three acceptance shapes:

  1. `phenofastmcp @ git+https://github.com/KooshaPari/PhenoFastMCP@<tag>`
  2. Any non-comment dependency line that contains the fork URL
  3. A `# phenofastmcp-fork-pin: <reason>` deviation directive

…and its rejection of comment-only "pin" lines, which is the exact
failure mode the audit (2026-09-05) called out for substrate:

    "# Pin to PhenoFastMCP when published:
     # phenofastmcp @ git+https://github.com/KooshaPari/PhenoFastMCP.git@v3.4.2"

A comment documents intent but does not constrain pip; the validator
must report it as an invalid pin so the gap is visible at CI time.
"""
from __future__ import annotations

import importlib.util
from pathlib import Path
from textwrap import dedent

import pytest

# Load the validator module from scripts/ without making `scripts/` a
# package — the file uses a `_` underscore on `__pyproject` paths and
# is invoked as a script, not imported, by CI.
_VALIDATOR_PATH = (
    Path(__file__).resolve().parent.parent / "scripts" / "validate_fork_pin.py"
)
_spec = importlib.util.spec_from_file_location("validate_fork_pin", _VALIDATOR_PATH)
assert _spec and _spec.loader, "could not load validate_fork_pin"
validate_fork_pin = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(validate_fork_pin)  # type: ignore[union-attr]


def _server(
    *,
    sid: str = "substrate",
    framework: str = "fastmcp",
    status: str = "active",
    package: str = "servers/substrate",
) -> dict:
    return {
        "id": sid,
        "title": "Test server",
        "framework": framework,
        "status": status,
        "package": package,
    }


# ---------------------------------------------------------------------------
# _package_hits — narrow unit test for the package-line scanner
# ---------------------------------------------------------------------------


def test_package_hits_skips_comment_lines() -> None:
    """A commented pin must not satisfy the scan — it's intent, not a
    real constraint."""
    text = dedent(
        """\
        # Pin to PhenoFastMCP when published:
        # phenofastmcp @ git+https://github.com/KooshaPari/PhenoFastMCP.git@v3.4.2
        fastmcp>=3.4.2
        httpx
        """
    )
    hits = validate_fork_pin._package_hits(text, validate_fork_pin.FORK_PKG_NAMES)
    assert hits == [], hits
    upstream = validate_fork_pin._package_hits(text, validate_fork_pin.UPSTREAM_PKG_NAMES)
    assert upstream == ["fastmcp>=3.4.2"]


def test_package_hits_finds_uncommented_fork_pin() -> None:
    text = dedent(
        """\
        dependencies = [
            "phenofastmcp @ git+https://github.com/KooshaPari/PhenoFastMCP.git@v3.4.2",
            "httpx",
        ]
        """
    )
    hits = validate_fork_pin._package_hits(text, validate_fork_pin.FORK_PKG_NAMES)
    # The hit preserves PEP 508 quoting/comma punctuation; we only
    # care that the package name is detected on a real dependency
    # line. Pinning the exact whitespace is brittle.
    assert len(hits) == 1
    assert "phenofastmcp" in hits[0]
    assert "git+https://github.com/KooshaPari/PhenoFastMCP" in hits[0]


# ---------------------------------------------------------------------------
# _has_fork_url — strict (non-comment lines only)
# ---------------------------------------------------------------------------


def test_fork_url_in_comment_only_is_rejected() -> None:
    text = dedent(
        """\
        # phenofastmcp @ git+https://github.com/KooshaPari/PhenoFastMCP.git@v3.4.2
        fastmcp>=3.4.2
        """
    )
    assert validate_fork_pin._has_fork_url(text) is False


def test_fork_url_on_dependency_line_accepted() -> None:
    text = (
        'phenofastmcp @ git+https://github.com/KooshaPari/PhenoFastMCP.git@v3.4.2'
    )
    assert validate_fork_pin._has_fork_url(text) is True


# ---------------------------------------------------------------------------
# _has_fork_directive — deviation comment format
# ---------------------------------------------------------------------------


def test_fork_directive_accepts_documented_deviation() -> None:
    text = (
        "# phenofastmcp-fork-pin: vendored mirror at "
        "git+https://internal.example.com/pheno-fastmcp@v3.4.2"
    )
    assert validate_fork_pin._has_fork_directive(text) is True


def test_fork_directive_rejects_unrelated_comment() -> None:
    text = "# this comment does not document the fork pin"
    assert validate_fork_pin._has_fork_directive(text) is False


# ---------------------------------------------------------------------------
# check_server — end-to-end on synthesized catalog entries
# ---------------------------------------------------------------------------


def test_check_server_accepts_uncommented_fork_pin(tmp_path: Path, monkeypatch) -> None:
    """A real dependency line pinning the fork URL is accepted."""
    # Point the validator at a tmp dir containing a server with a
    # proper fork pin. We don't actually invoke main(); we call
    # check_server directly so we don't need to write a full catalog.
    server_dir = tmp_path / "servers" / "substrate"
    server_dir.mkdir(parents=True)
    (server_dir / "pyproject.toml").write_text(
        dedent(
            """\
            [project]
            name = "phenomcp-server-substrate"
            dependencies = [
                "phenofastmcp @ git+https://github.com/KooshaPari/PhenoFastMCP.git@v3.4.2",
                "httpx",
            ]
            """
        )
    )

    monkeypatch.setattr(validate_fork_pin, "ROOT", tmp_path)
    errors = validate_fork_pin.check_server(_server())
    assert errors == [], errors


def test_check_server_rejects_comment_only_pin(tmp_path: Path, monkeypatch) -> None:
    """The exact substrate failure mode: comment-only pin, upstream dep."""
    server_dir = tmp_path / "servers" / "substrate"
    server_dir.mkdir(parents=True)
    (server_dir / "pyproject.toml").write_text(
        dedent(
            """\
            [project]
            name = "phenomcp-server-substrate"
            dependencies = [
                "fastmcp>=3.4.2",
                "httpx",
            ]
            # Pin to PhenoFastMCP when published:
            # phenofastmcp @ git+https://github.com/KooshaPari/PhenoFastMCP.git@v3.4.2
            """
        )
    )

    monkeypatch.setattr(validate_fork_pin, "ROOT", tmp_path)
    errors = validate_fork_pin.check_server(_server())
    assert len(errors) == 1, errors
    assert "no hard-fork pin" in errors[0]
    assert "'substrate'" in errors[0]
    assert "fastmcp>=3.4.2" in errors[0]


def test_check_server_accepts_documented_deviation_directive(
    tmp_path: Path, monkeypatch
) -> None:
    """A deviation directive is an acceptable escape hatch — it must
    name the fork and the reason."""
    server_dir = tmp_path / "servers" / "substrate"
    server_dir.mkdir(parents=True)
    (server_dir / "pyproject.toml").write_text(
        dedent(
            """\
            [project]
            name = "phenomcp-server-substrate"
            dependencies = [
                "fastmcp>=3.4.2",  # upstream intentional until fork tag stabilizes
            ]
            # phenofastmcp-fork-pin: pending — see issue #42 to switch to
            # phenofastmcp @ git+https://github.com/KooshaPari/PhenoFastMCP.git@v3.4.2
            """
        )
    )

    monkeypatch.setattr(validate_fork_pin, "ROOT", tmp_path)
    errors = validate_fork_pin.check_server(_server())
    assert errors == [], errors


def test_check_server_skips_non_fastmcp_servers() -> None:
    """Only `framework: fastmcp` servers are in scope."""
    errs = validate_fork_pin.check_server(_server(framework="rmcp"))
    assert errs == []


def test_check_server_skips_pointer_and_deprecated() -> None:
    """Pointer / deprecated entries live in upstream repos; their
    fork resolution is not enforced here."""
    for status in ("pointer", "deprecated"):
        errs = validate_fork_pin.check_server(_server(status=status))
        assert errs == [], (status, errs)

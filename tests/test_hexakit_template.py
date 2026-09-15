"""Smoke test: render the mcp-server template and confirm validate_catalog passes.

This test does NOT require pyyaml/jsonschema to be installed at the repo
root level. The renderer imports them only when present; if missing, the
test still verifies that the scaffolded files were produced.
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
RENDERER = ROOT / "templates" / "mcp-server" / "scripts" / "render.py"


@pytest.fixture()
def rendered_dir(tmp_path: Path) -> Path:
    out = tmp_path / "out"
    cmd = [
        sys.executable,
        str(RENDERER),
        "--id", "echo",
        "--title", "Echo MCP",
        "--description", "Echoes input back",
        "--env", "ECHO_TOKEN,ECHO_DB",
        "--out", str(out),
    ]
    result = subprocess.run(cmd, capture_output=True, text=True)
    assert result.returncode == 0, f"renderer failed:\nSTDOUT:\n{result.stdout}\nSTDERR:\n{result.stderr}"
    return out


def test_server_module_present(rendered_dir: Path) -> None:
    assert (rendered_dir / "servers" / "echo" / "echo_server.py").is_file()
    pyproject = (rendered_dir / "servers" / "echo" / "pyproject.toml").read_text()
    assert "phenomcp-server-echo" in pyproject
    assert "fastmcp" in pyproject


def test_skill_files_present(rendered_dir: Path) -> None:
    assert (rendered_dir / "skills" / "echo-skill" / "SKILL.md").is_file()
    assert (rendered_dir / "skills" / "echo-skill" / "skill.yaml").is_file()


def test_plugin_files_present(rendered_dir: Path) -> None:
    plugin = rendered_dir / "plugins" / "echo-bundle"
    assert (plugin / "plugin.yaml").is_file()
    mcp_json = json.loads((plugin / "mcp.json").read_text())
    assert "echo" in mcp_json["mcpServers"]
    env = mcp_json["mcpServers"]["echo"]["env"]
    assert env["ECHO_TOKEN"] == ""
    assert env["ECHO_DB"] == ""


def test_agent_file_present(rendered_dir: Path) -> None:
    assert (rendered_dir / "agents" / "echo-lead" / "agent.yaml").is_file()


def test_catalog_entry_renders(rendered_dir: Path) -> None:
    yaml = rendered_dir / "catalog_entry.rendered.yaml"
    assert yaml.is_file()
    body = yaml.read_text()
    assert "id: echo" in body
    assert "id: echo-skill" in body
    assert "id: echo-bundle" in body
    assert "id: echo-lead" in body
    assert "status: template" in body


def test_renderer_validates_against_schema(rendered_dir: Path) -> None:
    try:
        import yaml  # noqa: F401
        from jsonschema import Draft202012Validator  # noqa: F401
    except ImportError:
        pytest.skip("pyyaml/jsonschema not installed")

    schema_path = ROOT / "schemas" / "registry.schema.json"
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    defs = schema.get("$defs", {})
    rendered_yaml = rendered_dir / "catalog_entry.rendered.yaml"
    data = yaml.safe_load(rendered_yaml.read_text(encoding="utf-8"))
    for entry in data.get("servers", []):
        Draft202012Validator(defs["server"]).validate(entry)
    for section, def_name in (("skills", "skill"), ("plugins", "plugin"), ("agents", "agent")):
        for entry in data.get(section, []):
            Draft202012Validator(defs[def_name]).validate(entry)


def test_validate_catalog_script_passes(rendered_dir: Path) -> None:
    """Merge rendered entry into a copy of the real catalog and run validate_catalog.py."""
    if not shutil.which(sys.executable):
        pytest.skip("python not available")
    try:
        import yaml  # noqa: F401
        from jsonschema import Draft202012Validator  # noqa: F401
    except ImportError:
        pytest.skip("pyyaml/jsonschema not installed")

    import yaml as _yaml

    repo_root = ROOT
    catalog_src = repo_root / "catalog" / "registry.yaml"
    catalog = _yaml.safe_load(catalog_src.read_text(encoding="utf-8"))
    rendered = _yaml.safe_load(
        (rendered_dir / "catalog_entry.rendered.yaml").read_text(encoding="utf-8")
    )
    for section in ("servers", "skills", "plugins", "agents"):
        catalog.setdefault(section, []).extend(rendered.get(section, []))

    merged = rendered_dir / "registry.yaml"
    merged.write_text(_yaml.safe_dump(catalog, sort_keys=False), encoding="utf-8")
    out_repo = rendered_dir / "phenomcp"
    out_repo.mkdir(parents=True, exist_ok=True)
    shutil.copytree(repo_root / "schemas", out_repo / "schemas")
    shutil.copytree(repo_root / "scripts", out_repo / "scripts")
    (out_repo / "catalog").mkdir(parents=True, exist_ok=True)
    shutil.copy(merged, out_repo / "catalog" / "registry.yaml")
    for kind in ("servers", "skills", "plugins", "agents"):
        src_dir = repo_root / kind
        if src_dir.exists():
            shutil.copytree(src_dir, out_repo / kind, dirs_exist_ok=True)
    skill_dir = out_repo / "skills" / "echo-skill"
    skill_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy(rendered_dir / "skills" / "echo-skill" / "SKILL.md", skill_dir / "SKILL.md")
    shutil.copy(rendered_dir / "skills" / "echo-skill" / "skill.yaml", skill_dir / "skill.yaml")
    plugin_dir = out_repo / "plugins" / "echo-bundle"
    plugin_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy(rendered_dir / "plugins" / "echo-bundle" / "plugin.yaml", plugin_dir / "plugin.yaml")
    shutil.copy(rendered_dir / "plugins" / "echo-bundle" / "mcp.json", plugin_dir / "mcp.json")
    agent_dir = out_repo / "agents" / "echo-lead"
    agent_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy(rendered_dir / "agents" / "echo-lead" / "agent.yaml", agent_dir / "agent.yaml")
    server_dir = out_repo / "servers" / "echo"
    server_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy(rendered_dir / "servers" / "echo" / "echo_server.py", server_dir / "echo_server.py")
    shutil.copy(rendered_dir / "servers" / "echo" / "pyproject.toml", server_dir / "pyproject.toml")

    proc = subprocess.run(
        [sys.executable, "scripts/validate_catalog.py"],
        cwd=out_repo,
        capture_output=True,
        text=True,
    )
    assert proc.returncode == 0, f"validate_catalog failed:\nSTDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"

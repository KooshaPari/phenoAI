from __future__ import annotations

import shutil
from pathlib import Path

from .models import EnvDoctorReport


def _require_bin(name: str, missing: list[str], versions: dict[str, str]) -> None:
    found = shutil.which(name)
    if not found:
        missing.append(name)
        return
    versions[name] = found


def run_env_doctor(target: str, repo_checkouts: list[Path]) -> EnvDoctorReport:
    missing: list[str] = []
    versions: dict[str, str] = {}
    detected_files: list[str] = []

    for checkout in repo_checkouts:
        if (checkout / "Taskfile.yml").exists():
            detected_files.append(str(checkout / "Taskfile.yml"))
            _require_bin("task", missing, versions)

        if (checkout / "justfile").exists():
            detected_files.append(str(checkout / "justfile"))
            _require_bin("just", missing, versions)

        if (checkout / "Makefile").exists():
            detected_files.append(str(checkout / "Makefile"))
            _require_bin("make", missing, versions)

        if (checkout / "pyproject.toml").exists():
            detected_files.append(str(checkout / "pyproject.toml"))
            if shutil.which("uv"):
                _require_bin("uv", missing, versions)
            else:
                _require_bin("python", missing, versions)

        if (checkout / "Cargo.toml").exists():
            detected_files.append(str(checkout / "Cargo.toml"))
            _require_bin("cargo", missing, versions)

        if (checkout / "go.mod").exists():
            detected_files.append(str(checkout / "go.mod"))
            _require_bin("go", missing, versions)

        if (checkout / "package.json").exists():
            detected_files.append(str(checkout / "package.json"))
            if (checkout / "pnpm-lock.yaml").exists():
                _require_bin("pnpm", missing, versions)
            elif (checkout / "bun.lock").exists() or (checkout / "bun.lockb").exists():
                _require_bin("bun", missing, versions)
            elif (checkout / "yarn.lock").exists():
                _require_bin("yarn", missing, versions)
            else:
                _require_bin("npm", missing, versions)

    status = "pass" if not missing else "fail"
    return EnvDoctorReport(
        target_name=target,
        doctor_status=status,
        missing_requirements=sorted(set(missing)),
        resolved_versions=versions,
        detected_files=sorted(set(detected_files)),
    )

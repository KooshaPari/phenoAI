from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal


TargetMode = Literal["repo", "stack"]


@dataclass(slots=True)
class RepoSelection:
    repo_id: str
    repo_path: str
    selected_ref: str
    source_worktree_path: str | None = None
    resolved_sha: str | None = None


@dataclass(slots=True)
class TargetLock:
    schema_version: int
    target_name: str
    mode: TargetMode
    repos: list[RepoSelection] = field(default_factory=list)
    lock_hash: str = ""
    created_at_utc: str = ""


@dataclass(slots=True)
class RuntimeRepo:
    repo_id: str
    checkout_path: str
    resolved_sha: str
    head_branch: str | None = None


@dataclass(slots=True)
class RuntimeState:
    target_name: str
    materialized_root: str
    repo_materializations: list[RuntimeRepo] = field(default_factory=list)
    materialized_at_utc: str = ""


@dataclass(slots=True)
class RunnerCommand:
    runner: str
    name: str
    description: str
    source_file: str


@dataclass(slots=True)
class RunnerCatalog:
    target_name: str
    runners_detected: list[str] = field(default_factory=list)
    commands: list[RunnerCommand] = field(default_factory=list)
    default_command: str = ""


@dataclass(slots=True)
class EnvDoctorReport:
    target_name: str
    doctor_status: Literal["pass", "fail"]
    missing_requirements: list[str] = field(default_factory=list)
    resolved_versions: dict[str, str] = field(default_factory=dict)
    detected_files: list[str] = field(default_factory=list)

from __future__ import annotations

import os
from pathlib import Path


def phenotype_root() -> Path:
    configured = os.environ.get("THGENT_PHENOTYPE_ROOT")
    if configured:
        return Path(configured).expanduser().resolve()
    return Path("/Users/kooshapari/CodeProjects/Phenotype").resolve()


def projects_root() -> Path:
    return phenotype_root() / "projects"


def home_mirror_root() -> Path:
    configured = os.environ.get("THGENT_PHENCH_HOME_ROOT")
    if configured:
        return Path(configured).expanduser().resolve()
    return Path.home() / "phench"


def target_root(target: str) -> Path:
    return projects_root() / target


def target_repos_root(target: str) -> Path:
    return target_root(target) / "repos"


def target_state_root(target: str) -> Path:
    return target_root(target) / ".phench"


def mirror_target_state_root(target: str) -> Path:
    return home_mirror_root() / target / ".phench"

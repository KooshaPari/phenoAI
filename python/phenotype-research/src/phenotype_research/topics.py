"""TopicExtractor — derives research topics from pyproject.toml + manual config."""

from __future__ import annotations

import re
from pathlib import Path

from thegent.infra.fast_yaml_parser import yaml_load


class TopicExtractor:
    """Extract research topics from project deps + manual override YAML."""

    def __init__(
        self,
        project_root: Path,
        config_path: Path | None = None,
    ) -> None:
        self._root = Path(project_root)
        self._config = config_path or self._root / "research-topics.yaml"

    def extract(self) -> list[str]:
        """Extract topics from pyproject.toml and manual config, dedupped."""
        topics: list[str] = []
        topics.extend(self._from_pyproject())
        topics.extend(self._from_config())
        seen: set[str] = set()
        result = []
        for t in topics:
            if t not in seen:
                seen.add(t)
                result.append(t)
        return result

    def _from_pyproject(self) -> list[str]:
        """Extract topics from pyproject.toml dependencies."""
        path = self._root / "pyproject.toml"
        if not path.exists():
            return []
        text = path.read_text()
        return re.findall(r'"([a-zA-Z][a-zA-Z0-9_-]+)(?:[>=<!\[,]|")', text)

    def _from_config(self) -> list[str]:
        """Extract topics from manual research-topics.yaml config."""
        if not self._config.exists():
            return []
        data = yaml_load(self._config.read_text())
        return [str(t) for t in (data or {}).get("topics", [])]

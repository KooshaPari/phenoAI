"""BaseCrawler ABC - all source crawlers inherit from this."""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from research_engine.schema import ResearchItem

Tier = Literal["hourly", "daily", "weekly"]


class BaseCrawler(ABC):
    """Abstract base class for all crawler implementations.

    Concrete crawlers must define:
    - source: str (e.g., "hn", "reddit", "arxiv")
    - tier: Tier (hourly, daily, or weekly)
    - fetch(topics): list[ResearchItem]

    All errors (network, parse, validation) propagate immediately. Fail fast, fail loudly.
    """

    source: str
    tier: Tier

    @abstractmethod
    def fetch(self, topics: list[str]) -> list[ResearchItem]:
        """Fetch research items matching topics.

        Args:
            topics: List of topic keywords to search for.

        Returns:
            List of ResearchItem objects.

        Raises:
            Any exceptions from network/parse/validation failures propagate immediately.
            No fallbacks, no silent failures.
        """
        ...

"""CrawlerRegistry - holds all registered crawler instances."""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from research_engine.crawlers.base import BaseCrawler, Tier


class CrawlerRegistry:
    """Registry of crawler instances, queryable by tier.

    Thread-safe operations are provided by the caller; this class itself
    is not internally synchronized. Registrations should happen early
    (e.g., at app startup), not dynamically during concurrent fetch operations.
    """

    def __init__(self) -> None:
        """Initialize empty crawler registry."""
        self._crawlers: list[BaseCrawler] = []

    def register(self, crawler: BaseCrawler) -> None:
        """Register a crawler instance.

        Args:
            crawler: BaseCrawler instance to register.
        """
        self._crawlers.append(crawler)

    def get_by_tier(self, tier: Tier) -> list[BaseCrawler]:
        """Get all crawlers matching a tier.

        Args:
            tier: Tier level ("hourly", "daily", or "weekly").

        Returns:
            List of crawlers with matching tier (may be empty).
        """
        return [c for c in self._crawlers if c.tier == tier]

    def get_all(self) -> list[BaseCrawler]:
        """Get all registered crawlers.

        Returns:
            List of all registered crawlers (may be empty).
        """
        return list(self._crawlers)

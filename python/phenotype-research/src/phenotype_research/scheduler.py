"""TieredScheduler — APScheduler-based job orchestration for research crawlers.

Manages hourly, daily, and weekly fetch cycles. Fail-fast, no silent failures.
All exceptions propagate. No fallbacks, no optional degradation.
"""

from __future__ import annotations

from pathlib import Path

import structlog
from apscheduler.schedulers.background import BackgroundScheduler

from research_engine.crawlers.registry import CrawlerRegistry
from research_engine.store import ResearchStore

log = structlog.get_logger(__name__)


class TieredScheduler:
    """Orchestrates tiered crawler execution with persistent storage.

    Manages hourly, daily, and weekly fetch jobs via APScheduler.
    All crawlers in a given tier run sequentially; results are upserted
    to the research store. Network/parse/validation errors propagate immediately.
    """

    def __init__(self, db_path: Path, topics: list[str]) -> None:
        """Initialize scheduler.

        Args:
            db_path: Path to SQLite research database.
            topics: List of topics to search for across all crawlers.

        Raises:
            Any exceptions from store initialization propagate immediately.
        """
        self._db_path = Path(db_path)
        self._topics = topics
        self.registry = CrawlerRegistry()
        self._store = ResearchStore(self._db_path)
        self._scheduler = BackgroundScheduler()

    def start(self) -> None:
        """Start the scheduler with tiered job definitions.

        Registers three jobs (hourly, daily, weekly) and starts APScheduler.
        This blocks background threads; only one scheduler should be active per process.

        Raises:
            Any exceptions from APScheduler propagate immediately.
        """
        self._scheduler.add_job(
            self._run_tier,
            "interval",
            hours=1,
            args=["hourly"],
            id="hourly",
        )
        self._scheduler.add_job(
            self._run_tier,
            "interval",
            hours=24,
            args=["daily"],
            id="daily",
        )
        self._scheduler.add_job(
            self._run_tier,
            "interval",
            weeks=1,
            args=["weekly"],
            id="weekly",
        )
        self._scheduler.start()
        log.info("scheduler.started", tiers=["hourly", "daily", "weekly"])

    def stop(self) -> None:
        """Stop the scheduler gracefully.

        Shuts down APScheduler without waiting for pending jobs.
        Safe to call if scheduler is not running.
        """
        if self._scheduler.running:
            self._scheduler.shutdown(wait=False)
            log.info("scheduler.stopped")

    def _run_tier(self, tier: str) -> None:
        """Execute all crawlers registered for the given tier.

        Fetches items from each crawler sequentially, upserts to store.
        Network/parse/validation errors from any crawler propagate immediately.

        Args:
            tier: One of "hourly", "daily", or "weekly".

        Raises:
            Any exceptions from crawlers or store operations propagate immediately.
        """
        crawlers = self.registry.get_by_tier(tier)  # type: ignore[arg-type]
        for crawler in crawlers:
            items = crawler.fetch(self._topics)
            for item in items:
                self._store.upsert(item)
            log.info("crawler.done", source=crawler.source, count=len(items))

"""RSS/Atom feed crawler via feedparser."""

from __future__ import annotations

from datetime import UTC, datetime

import feedparser

from research_engine.crawlers.base import BaseCrawler
from research_engine.crawlers.hn import _relevance
from research_engine.schema import ResearchItem

DEFAULT_FEEDS = [
    "https://blog.python.org/feeds/posts/default",
    "https://realpython.com/atom.xml",
    "https://simonwillison.net/atom/everything/",
]


class RSSCrawler(BaseCrawler):
    """RSS/Atom feed crawler."""

    source = "rss"
    tier = "weekly"

    def __init__(self, feeds: list[str] | None = None) -> None:
        """Initialize RSS crawler with optional feed URLs.

        Args:
            feeds: List of RSS/Atom feed URLs to crawl. Uses defaults if None.
        """
        self._feeds = feeds or DEFAULT_FEEDS

    def fetch(self, topics: list[str]) -> list[ResearchItem]:
        """Fetch entries from RSS/Atom feeds matching topics.

        Args:
            topics: List of topic keywords to search for.

        Returns:
            List of ResearchItem objects from feeds.

        Raises:
            feedparser exceptions on network/parse failures.
        """
        now = datetime.now(UTC)
        items = []
        for feed_url in self._feeds:
            feed = feedparser.parse(feed_url)
            for entry in feed.entries[:10]:
                title = getattr(entry, "title", "")
                summary = getattr(entry, "summary", "")[:500]
                url = getattr(entry, "link", feed_url)
                items.append(
                    ResearchItem.from_url(
                        url=url,
                        source="rss",
                        title=title,
                        summary=summary,
                        score=0,
                        tags=[t for t in topics if t.lower() in (title + summary).lower()],
                        fetched_at=now,
                        relevance=_relevance(title, summary, topics),
                    )
                )
        return items

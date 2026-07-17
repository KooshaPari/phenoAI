"""Hacker News crawler via Algolia HN Search API."""

from __future__ import annotations

from datetime import UTC, datetime

import httpx

from research_engine.crawlers.base import BaseCrawler
from research_engine.schema import ResearchItem

_HN_API = "https://hn.algolia.com/api/v1/search"


def _relevance(title: str, summary: str, topics: list[str]) -> float:
    """Calculate relevance score based on topic matches."""
    text = (title + " " + summary).lower()
    matches = sum(1 for t in topics if t.lower() in text)
    return min(1.0, matches / max(len(topics), 1))


class HNCrawler(BaseCrawler):
    """Hacker News crawler using Algolia API."""

    source = "hn"
    tier = "hourly"

    def fetch(self, topics: list[str]) -> list[ResearchItem]:
        """Fetch stories from Hacker News matching topics.

        Args:
            topics: List of topic keywords to search for.

        Returns:
            List of ResearchItem objects from HN.

        Raises:
            httpx.HTTPError: If API request fails.
            KeyError: If response format is unexpected.
        """
        query = " OR ".join(topics[:5])
        resp = httpx.get(_HN_API, params={"query": query, "tags": "story", "hitsPerPage": 30})
        resp.raise_for_status()
        hits = resp.json().get("hits", [])
        now = datetime.now(UTC)
        items = []
        for hit in hits:
            url = hit.get("url") or f"https://news.ycombinator.com/item?id={hit['objectID']}"
            title = hit.get("title", "")
            summary = hit.get("story_text") or hit.get("comment_text") or ""
            items.append(
                ResearchItem.from_url(
                    url=url,
                    source="hn",
                    title=title,
                    summary=summary[:500],
                    score=hit.get("points") or 0,
                    tags=[t for t in topics if t.lower() in (title + summary).lower()],
                    fetched_at=now,
                    relevance=_relevance(title, summary, topics),
                )
            )
        return items

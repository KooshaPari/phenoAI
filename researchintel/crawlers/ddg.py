"""DuckDuckGo instant answer crawler."""

from __future__ import annotations

from datetime import UTC, datetime

import httpx

from researchintel.crawlers.base import BaseCrawler
from researchintel.crawlers.hn import _relevance
from researchintel.schema import ResearchItem

_DDG_API = "https://api.duckduckgo.com/"


class DDGCrawler(BaseCrawler):
    """DuckDuckGo instant answer and search results crawler."""

    source = "ddg"
    tier = "daily"

    def fetch(self, topics: list[str]) -> list[ResearchItem]:
        """Fetch related topics from DuckDuckGo API."""
        query = " ".join(topics[:4])
        resp = httpx.get(_DDG_API, params={"q": query, "format": "json", "no_html": 1})
        resp.raise_for_status()
        data = resp.json()
        now = datetime.now(UTC)
        items = []
        for topic in data.get("RelatedTopics", [])[:20]:
            url = topic.get("FirstURL", "")
            text = topic.get("Text", "")
            if not url:
                continue
            items.append(
                ResearchItem.from_url(
                    url=url,
                    source="ddg",
                    title=text[:80],
                    summary=text[:500],
                    score=0,
                    tags=[t for t in topics if t.lower() in text.lower()],
                    fetched_at=now,
                    relevance=_relevance(text, text, topics),
                )
            )
        return items

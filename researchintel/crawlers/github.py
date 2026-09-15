"""GitHub search crawler."""

from __future__ import annotations

from datetime import UTC, datetime

import httpx

from researchintel.crawlers.base import BaseCrawler
from researchintel.crawlers.hn import _relevance
from researchintel.schema import ResearchItem

_GH_SEARCH = "https://api.github.com/search/repositories"


class GitHubCrawler(BaseCrawler):
    """GitHub repository search crawler."""

    source = "github"
    tier = "daily"

    def __init__(self, token: str | None = None) -> None:
        """Initialize GitHub crawler with optional authentication."""
        self._headers = {"Authorization": f"Bearer {token}"} if token else {}

    def fetch(self, topics: list[str]) -> list[ResearchItem]:
        """Fetch repositories from GitHub matching topics."""
        query = "+".join(topics[:4])
        resp = httpx.get(
            _GH_SEARCH,
            params={"q": query, "sort": "stars", "per_page": 20},
            headers=self._headers,
        )
        resp.raise_for_status()
        now = datetime.now(UTC)
        items = []
        for repo in resp.json().get("items", []):
            title = repo["full_name"]
            summary = repo.get("description") or ""
            items.append(
                ResearchItem.from_url(
                    url=repo["html_url"],
                    source="github",
                    title=title,
                    summary=summary,
                    score=repo["stargazers_count"],
                    tags=[
                        t
                        for t in topics
                        if t.lower() in (title + summary).lower()
                    ],
                    fetched_at=now,
                    relevance=_relevance(title, summary, topics),
                )
            )
        return items

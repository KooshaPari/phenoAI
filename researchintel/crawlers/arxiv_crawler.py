"""arXiv crawler via arxiv Python library."""

from __future__ import annotations

from datetime import UTC, datetime

import arxiv

from researchintel.crawlers.base import BaseCrawler
from researchintel.crawlers.hn import _relevance
from researchintel.schema import ResearchItem


class ArxivCrawler(BaseCrawler):
    """arXiv crawler using arxiv Python library."""

    source = "arxiv"
    tier = "daily"

    def fetch(self, topics: list[str]) -> list[ResearchItem]:
        """Fetch papers from arXiv matching topics."""
        query = " AND ".join(f'ti:"{t}"' for t in topics[:3])
        now = datetime.now(UTC)
        search = arxiv.Search(
            query=query,
            max_results=20,
            sort_by=arxiv.SortCriterion.SubmittedDate,
        )
        items = []
        for result in search.results():
            title = result.title
            summary = result.summary[:500]
            items.append(
                ResearchItem.from_url(
                    url=result.entry_id,
                    source="arxiv",
                    title=title,
                    summary=summary,
                    score=0,
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

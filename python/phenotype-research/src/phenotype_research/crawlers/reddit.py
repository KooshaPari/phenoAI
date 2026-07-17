"""Reddit crawler via PRAW."""

from __future__ import annotations

from datetime import UTC, datetime

import praw

from research_engine.crawlers.base import BaseCrawler
from research_engine.crawlers.hn import _relevance
from research_engine.schema import ResearchItem

_SUBREDDITS = ["Python", "MachineLearning", "programming", "devops", "artificial", "LocalLLaMA"]


class RedditCrawler(BaseCrawler):
    """Reddit crawler using PRAW."""

    source = "reddit"
    tier = "hourly"

    def __init__(self, client_id: str, client_secret: str, user_agent: str) -> None:
        """Initialize Reddit crawler with authentication.

        Args:
            client_id: Reddit OAuth client ID.
            client_secret: Reddit OAuth client secret.
            user_agent: User agent string for API requests.
        """
        self._reddit = praw.Reddit(
            client_id=client_id,
            client_secret=client_secret,
            user_agent=user_agent,
            check_for_async=False,
        )

    def fetch(self, topics: list[str]) -> list[ResearchItem]:
        """Fetch submissions from Reddit matching topics.

        Args:
            topics: List of topic keywords to search for.

        Returns:
            List of ResearchItem objects from Reddit.

        Raises:
            praw.exceptions.PrawException: If API request fails.
        """
        query = " OR ".join(topics[:5])
        now = datetime.now(UTC)
        items: list[ResearchItem] = []
        for name in _SUBREDDITS:
            for sub in self._reddit.subreddit(name).search(
                query, limit=10, sort="top", time_filter="day"
            ):
                title = sub.title
                summary = (sub.selftext or "")[:500]
                items.append(
                    ResearchItem.from_url(
                        url=sub.url,
                        source="reddit",
                        title=title,
                        summary=summary,
                        score=sub.score,
                        tags=[t for t in topics if t.lower() in (title + summary).lower()],
                        fetched_at=now,
                        relevance=_relevance(title, summary, topics),
                    )
                )
        return items

"""MCP tools for research_engine — 6 tools. @trace FR-RES-031"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import structlog

log = structlog.get_logger(__name__)

_GLOBAL_DB = Path.home() / ".thegent" / "research.db"


def register_tools(mcp: Any) -> tuple:
    """Register all 6 research MCP tools. Returns tuple so pyright sees references.

    Args:
        mcp: FastMCP server instance.

    Returns:
        Tuple of 6 registered tool functions.
    """

    @mcp.tool("thegent_research_search")
    def thegent_research_search(query: str, limit: int = 20) -> str:
        """Search research items by keyword.

        Args:
            query: Search string (searches title and summary).
            limit: Maximum results to return.

        Returns:
            Markdown list of matching items.
        """
        from research_engine.store import ResearchStore

        store = ResearchStore(_GLOBAL_DB)
        items = store.search(query)[:limit]
        if not items:
            return "No results found."
        lines = [f"- [{i.title}]({i.url}) ({i.source}, score {i.score})" for i in items]
        return "\n".join(lines)

    @mcp.tool("thegent_research_recent")
    def thegent_research_recent(hours: int = 24, limit: int = 20) -> str:
        """Get recent research items.

        Args:
            hours: Look back this many hours.
            limit: Maximum items to return.

        Returns:
            Markdown list of recent items.
        """
        from research_engine.store import ResearchStore

        store = ResearchStore(_GLOBAL_DB)
        items = store.get_recent(hours=hours, limit=limit)
        if not items:
            return "No recent items."
        lines = [f"- [{i.title}]({i.url}) ({i.source}, relevance {i.relevance:.0%})" for i in items]
        return "\n".join(lines)

    @mcp.tool("thegent_research_digest")
    def thegent_research_digest(hours: int = 24, limit: int = 20) -> str:
        """Generate a research digest in markdown.

        Args:
            hours: Look back this many hours.
            limit: Maximum items to include.

        Returns:
            Markdown digest of research items.
        """
        from research_engine.digest import DigestGenerator
        from research_engine.store import ResearchStore

        store = ResearchStore(_GLOBAL_DB)
        return DigestGenerator(store).generate(hours=hours, limit=limit)

    @mcp.tool("thegent_research_crawl")
    def thegent_research_crawl(topics: list[str] | None = None) -> str:
        """Trigger an immediate crawl of all sources.

        Args:
            topics: Topics to crawl for. If None, auto-detect from project.

        Returns:
            String with count of items crawled.
        """
        from pathlib import Path

        from research_engine.crawlers.registry import CrawlerRegistry
        from research_engine.store import ResearchStore
        from research_engine.topics import TopicExtractor

        store = ResearchStore(_GLOBAL_DB)
        if topics is None:
            topics = TopicExtractor(project_root=Path.cwd()).extract()
        registry = CrawlerRegistry()
        count = 0
        for crawler in registry.get_all():
            for item in crawler.fetch(topics):
                store.upsert(item)
                count += 1
        return f"Crawled {count} items."

    @mcp.tool("thegent_research_topics")
    def thegent_research_topics() -> str:
        """List detected project topics.

        Returns:
            Markdown list of detected topics.
        """
        from research_engine.topics import TopicExtractor

        topics = TopicExtractor(project_root=Path.cwd()).extract()
        return "\\n".join(f"- {t}" for t in topics) if topics else "No topics detected."

    @mcp.tool("thegent_research_sync")
    def thegent_research_sync(project_db: str, min_relevance: float = 0.5) -> str:
        """Sync global research DB to project-local DB.

        Args:
            project_db: Path to project database.
            min_relevance: Minimum relevance threshold for items to sync.

        Returns:
            String with count of synced items.
        """
        from research_engine.store import ResearchStore

        store = ResearchStore(_GLOBAL_DB)
        n = store.mirror_to_project(Path(project_db), min_relevance=min_relevance)
        return f"Synced {n} items to {project_db}."

    return (
        thegent_research_search,
        thegent_research_recent,
        thegent_research_digest,
        thegent_research_crawl,
        thegent_research_topics,
        thegent_research_sync,
    )

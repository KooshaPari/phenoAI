"""Session hook for injecting research context into agent sessions. @trace FR-RES-030"""

from __future__ import annotations

from typing import TYPE_CHECKING

import structlog

if TYPE_CHECKING:
    from research_engine.store import ResearchStore

log = structlog.get_logger(__name__)


def inject_session_context(store: ResearchStore, *, hours: int = 24, limit: int = 10) -> str:
    """Return a markdown snippet of recent research to inject into session context.

    Args:
        store: ResearchStore instance to query.
        hours: Look back this many hours. Defaults to 24.
        limit: Maximum items to return. Defaults to 10.

    Returns:
        Markdown-formatted string of recent research context.
    """
    items = store.get_recent(hours=hours, limit=limit)
    if not items:
        return "## Recent Research\n\nNo recent research items available.\n"

    lines = ["## Recent Research Context", ""]
    for item in items:
        tag_str = ", ".join(item.tags) if item.tags else "—"
        lines.append(
            f"- **[{item.title}]({item.url})** ({item.source}) — score {item.score}, relevance {item.relevance:.0%}"
        )
        lines.append(f"  Tags: {tag_str}")
        lines.append(f"  {item.summary[:200]}")
        lines.append("")
    return "\n".join(lines)

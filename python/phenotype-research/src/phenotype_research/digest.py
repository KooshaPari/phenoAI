"""DigestGenerator — renders recent research items as markdown digest."""

from __future__ import annotations

from collections import defaultdict
from datetime import UTC, datetime
from typing import Any

from research_engine.store import ResearchStore


class DigestGenerator:
    """Generate markdown digests from recent research items."""

    def __init__(self, store: ResearchStore) -> None:
        """Initialize with a research store.

        Args:
            store: ResearchStore instance to read items from.
        """
        self._store = store

    def generate(self, *, hours: int = 24, limit: int = 20) -> str:
        """Generate markdown digest of recent research items.

        Args:
            hours: Look back this many hours. Defaults to 24.
            limit: Maximum items to include. Defaults to 20.

        Returns:
            Markdown string with digest header and formatted items.
        """
        items = self._store.get_recent(hours=hours, limit=limit)
        now = datetime.now(UTC).strftime("%Y-%m-%d %H:%M UTC")
        lines = [f"## Research Digest — {now}\n"]

        if not items:
            lines.append(f"_No new items in the last {hours}h._\n")
            return "\n".join(lines)

        for item in items:
            # Build metadata line
            score_str = f"⭐ {item.score}" if item.score else ""
            tags_str = " ".join(f"`{t}`" for t in item.tags[:3])

            # Add item entry
            lines.append(f"### [{item.title}]({item.url})")
            metadata_parts = [f"**Source:** {item.source}"]
            if score_str:
                metadata_parts.append(score_str)
            metadata_parts.append(f"**Relevance:** {item.relevance:.0%}")
            if tags_str:
                metadata_parts.append(tags_str)
            lines.append(" | ".join(metadata_parts))

            # Add summary if present
            if item.summary:
                lines.append(f"> {item.summary[:200]}")
            lines.append("")

        return "\n".join(lines)


def build_hourly_change_digest(events: list[dict[str, Any]]) -> dict[str, Any]:
    """Build an hourly digest grouped by connector, action, and outcome."""
    buckets: dict[str, dict[str, dict[str, int]]] = defaultdict(
        lambda: defaultdict(lambda: defaultdict(int))
    )
    for event in events:
        timestamp = str(event.get("timestamp", "")).strip()
        connector = str(event.get("connector", "unknown")).strip() or "unknown"
        action = str(event.get("action", "unknown")).strip() or "unknown"
        outcome = str(event.get("outcome", "unknown")).strip() or "unknown"
        count = int(event.get("count", 1)) if "count" in event else 1
        if count <= 0:
            continue
        if not timestamp:
            raise ValueError("each event must include a timestamp")
        hour_bucket = datetime.fromisoformat(timestamp.replace("Z", "+00:00")).strftime(
            "%Y-%m-%dT%H:00:00Z"
        )
        buckets[hour_bucket][connector][f"{action}:{outcome}"] += count

    normalized: dict[str, Any] = {}
    for hour in sorted(buckets):
        normalized[hour] = {}
        for connector in sorted(buckets[hour]):
            normalized[hour][connector] = dict(sorted(buckets[hour][connector].items()))

    return {"bucket": "hourly", "hours": normalized}

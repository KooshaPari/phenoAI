"""ResearchStore — SQLite persistence for research items."""

from __future__ import annotations

import sqlite3
from datetime import UTC, datetime, timedelta
from pathlib import Path

import orjson as json

from research_engine.schema import ResearchItem


class ResearchStore:
    """SQLite store for ResearchItem objects.

    Provides upsert, search, get_recent, and mirror operations.
    Uses ROWID for row factory for all queries.
    """

    def __init__(self, db_path: Path) -> None:
        """Initialize store and create schema if needed.

        Args:
            db_path: Path to SQLite database file.
        """
        self._path = Path(db_path)
        self._path.parent.mkdir(parents=True, exist_ok=True)
        with self._connect() as conn:
            conn.executescript(ResearchItem.DDL)

    def _connect(self) -> sqlite3.Connection:
        """Create and return a database connection.

        Returns:
            sqlite3.Connection with Row factory configured.
        """
        conn = sqlite3.connect(self._path)
        conn.row_factory = sqlite3.Row
        return conn

    def upsert(self, item: ResearchItem) -> None:
        """Upsert a research item into the store.

        Inserts or updates based on slug (unique key).
        Score, relevance, and fetched_at are updated on conflict.

        Args:
            item: ResearchItem to insert or update.
        """
        with self._connect() as conn:
            conn.execute(
                """
                INSERT INTO research_items
                    (slug, source, url, title, summary, score, tags, fetched_at, relevance)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(slug) DO UPDATE SET
                    score=excluded.score,
                    relevance=excluded.relevance,
                    fetched_at=excluded.fetched_at
                """,
                (
                    item.slug,
                    item.source,
                    item.url,
                    item.title,
                    item.summary,
                    item.score,
                    json.dumps(item.tags).decode(),
                    item.fetched_at.isoformat(),
                    item.relevance,
                ),
            )
            conn.commit()

    def get_recent(self, *, hours: int = 24, limit: int = 50) -> list[ResearchItem]:
        """Get recent items fetched within the past N hours.

        Sorted by relevance (descending), then score (descending).

        Args:
            hours: Look back this many hours. Defaults to 24.
            limit: Maximum items to return. Defaults to 50.

        Returns:
            List of ResearchItem objects, sorted by relevance and score.
        """
        cutoff = (datetime.now(UTC) - timedelta(hours=hours)).isoformat()
        with self._connect() as conn:
            rows = conn.execute(
                "SELECT * FROM research_items WHERE fetched_at >= ? ORDER BY relevance DESC, score DESC LIMIT ?",
                (cutoff, limit),
            ).fetchall()
        return [self._row_to_item(r) for r in rows]

    def search(self, query: str, limit: int = 20) -> list[ResearchItem]:
        """Search items by title or summary (partial match).

        Args:
            query: Search string (case-insensitive partial match).
            limit: Maximum results to return. Defaults to 20.

        Returns:
            List of matching ResearchItem objects, sorted by relevance and score.
        """
        pattern = f"%{query}%"
        with self._connect() as conn:
            rows = conn.execute(
                "SELECT * FROM research_items WHERE title LIKE ? OR summary LIKE ? ORDER BY relevance DESC, score DESC LIMIT ?",
                (pattern, pattern, limit),
            ).fetchall()
        return [self._row_to_item(r) for r in rows]

    def mirror_to_project(self, project_db: Path, *, min_relevance: float = 0.3) -> int:
        """Mirror items with sufficient relevance to a project database.

        Creates a new project database and copies all items with
        relevance >= min_relevance.

        Args:
            project_db: Path to create project database at.
            min_relevance: Only copy items with relevance >= this value. Defaults to 0.3.

        Returns:
            Number of items copied.
        """
        items = self._get_by_relevance(min_relevance)
        if not items:
            return 0
        target = ResearchStore(project_db)
        for item in items:
            target.upsert(item)
        return len(items)

    def _get_by_relevance(self, min_relevance: float) -> list[ResearchItem]:
        """Get all items meeting minimum relevance threshold.

        Args:
            min_relevance: Minimum relevance score (0.0-1.0).

        Returns:
            List of ResearchItem objects sorted by relevance descending.
        """
        with self._connect() as conn:
            rows = conn.execute(
                "SELECT * FROM research_items WHERE relevance >= ? ORDER BY relevance DESC",
                (min_relevance,),
            ).fetchall()
        return [self._row_to_item(r) for r in rows]

    @staticmethod
    def _row_to_item(row: sqlite3.Row) -> ResearchItem:
        """Convert a database row to a ResearchItem.

        Args:
            row: sqlite3.Row from SELECT query.

        Returns:
            ResearchItem reconstructed from row data.
        """
        return ResearchItem(
            slug=row["slug"],
            source=row["source"],
            url=row["url"],
            title=row["title"],
            summary=row["summary"],
            score=row["score"],
            tags=json.loads(row["tags"]),
            fetched_at=datetime.fromisoformat(row["fetched_at"]),
            relevance=row["relevance"],
        )

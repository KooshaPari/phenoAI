"""ResearchItem Pydantic model — canonical data unit for all crawlers."""

from __future__ import annotations

import hashlib
from datetime import datetime
from typing import ClassVar, Literal

from pydantic import BaseModel, Field

Source = Literal[
    "hn",
    "reddit",
    "x",
    "arxiv",
    "github",
    "scholar",
    "pypi",
    "npm",
    "crates",
    "rss",
    "ddg",
    "other",
]


class ResearchItem(BaseModel):
    """Single research finding from any source."""

    slug: str = Field(description="sha256[:12] of url — dedup key")
    source: Source
    url: str
    title: str
    summary: str
    score: int = Field(default=0, description="upvotes / stars / citations")
    tags: list[str] = Field(default_factory=list, description="matched topic tags")
    fetched_at: datetime
    relevance: float = Field(default=0.0, ge=0.0, le=1.0, description="topic match score")

    # SQLite DDL — shared across ResearchStore instances
    DDL: ClassVar[str] = """
        CREATE TABLE IF NOT EXISTS research_items (
            slug        TEXT PRIMARY KEY,
            source      TEXT NOT NULL,
            url         TEXT NOT NULL,
            title       TEXT NOT NULL,
            summary     TEXT NOT NULL,
            score       INTEGER NOT NULL DEFAULT 0,
            tags        TEXT NOT NULL DEFAULT '[]',
            fetched_at  TEXT NOT NULL,
            relevance   REAL NOT NULL DEFAULT 0.0
        );
        CREATE INDEX IF NOT EXISTS idx_source ON research_items (source);
        CREATE INDEX IF NOT EXISTS idx_fetched_at ON research_items (fetched_at DESC);
        CREATE INDEX IF NOT EXISTS idx_relevance ON research_items (relevance DESC);
    """

    @classmethod
    def from_url(cls, *, url: str, source: Source, **kwargs) -> ResearchItem:
        slug = hashlib.sha256(url.encode()).hexdigest()[:12]
        return cls(slug=slug, source=source, url=url, **kwargs)

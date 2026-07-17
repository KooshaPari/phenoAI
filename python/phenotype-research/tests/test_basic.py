"""Basic tests for research_engine core modules.

Covers ResearchItem model, DigestGenerator, TopicExtractor,
ResearchStore (in-memory), session_hook, and build_hourly_change_digest.
"""

from __future__ import annotations

from datetime import UTC, datetime
from pathlib import Path
from tempfile import TemporaryDirectory

import pytest

from research_engine.digest import DigestGenerator, build_hourly_change_digest
from research_engine.schema import ResearchItem
from research_engine.session_hook import inject_session_context
from research_engine.store import ResearchStore
from research_engine.topics import TopicExtractor


class TestResearchItem:
    """ResearchItem model construction and validation."""

    def test_minimal_construction(self) -> None:
        item = ResearchItem(
            slug="test123",
            source="hn",
            url="https://example.com",
            title="Test Item",
            summary="A test summary",
            fetched_at=datetime.now(UTC),
        )
        assert item.slug == "test123"
        assert item.source == "hn"
        assert item.score == 0
        assert item.relevance == 0.0
        assert item.tags == []

    def test_from_url_creates_slug(self) -> None:
        item = ResearchItem.from_url(
            url="https://example.com/article",
            source="hn",
            title="Test",
            summary="Summary",
            fetched_at=datetime.now(UTC),
        )
        assert len(item.slug) == 12  # sha256[:12]
        assert item.url == "https://example.com/article"
        assert item.source == "hn"

    def test_source_literal_valid(self) -> None:
        for src in (
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
        ):
            item = ResearchItem(
                slug="s",
                source=src,  # type: ignore[arg-type]
                url="https://example.com",
                title="T",
                summary="S",
                fetched_at=datetime.now(UTC),
            )
            assert item.source == src

    def test_relevance_range(self) -> None:
        item = ResearchItem(
            slug="r",
            source="hn",
            url="https://example.com",
            title="T",
            summary="S",
            relevance=0.75,
            fetched_at=datetime.now(UTC),
        )
        assert 0.0 <= item.relevance <= 1.0


class TestDigestGenerator:
    """DigestGenerator with a real (temporary) in-memory store."""

    def test_empty_digest(self) -> None:
        with TemporaryDirectory() as tmp:
            store = ResearchStore(Path(tmp) / "test.db")
            gen = DigestGenerator(store)
            result = gen.generate(hours=24, limit=10)
        assert "Research Digest" in result
        assert "No new items" in result

    def test_digest_with_items(self) -> None:
        with TemporaryDirectory() as tmp:
            store = ResearchStore(Path(tmp) / "test.db")
            store.upsert(
                ResearchItem(
                    slug="a1b2c3d4e5f6",
                    source="github",
                    url="https://github.com/example/repo",
                    title="Example Repo",
                    summary="A sample repository for testing.",
                    score=42,
                    tags=["python", "testing"],
                    fetched_at=datetime.now(UTC),
                    relevance=0.9,
                )
            )
            gen = DigestGenerator(store)
            result = gen.generate(hours=24, limit=10)
        assert "Example Repo" in result
        assert "github" in result
        assert "42" in result or "42" in result

    def test_hours_filter(self) -> None:
        with TemporaryDirectory() as tmp:
            store = ResearchStore(Path(tmp) / "test.db")
            store.upsert(
                ResearchItem(
                    slug="olditem12345",
                    source="hn",
                    url="https://example.com/old",
                    title="Old Item",
                    summary="An old item.",
                    fetched_at=datetime(2020, 1, 1, tzinfo=UTC),
                    relevance=0.5,
                )
            )
            gen = DigestGenerator(store)
            # hours=1 should exclude the 2020 item
            result = gen.generate(hours=1, limit=10)
        assert "No new items" in result


class TestBuildHourlyChangeDigest:
    """build_hourly_change_digest edge cases."""

    def test_empty_events(self) -> None:
        result = build_hourly_change_digest([])
        assert result == {"bucket": "hourly", "hours": {}}

    def test_single_event(self) -> None:
        events = [
            {
                "timestamp": "2026-06-29T12:00:00Z",
                "connector": "github",
                "action": "fetch",
                "outcome": "success",
                "count": 5,
            }
        ]
        result = build_hourly_change_digest(events)
        assert result["bucket"] == "hourly"
        assert "2026-06-29T12:00:00Z" in result["hours"]
        assert result["hours"]["2026-06-29T12:00:00Z"]["github"]["fetch:success"] == 5

    def test_invalid_count_skipped(self) -> None:
        events = [
            {
                "timestamp": "2026-06-29T12:00:00Z",
                "connector": "github",
                "action": "fetch",
                "outcome": "success",
                "count": 0,
            },
            {
                "timestamp": "2026-06-29T12:00:00Z",
                "connector": "github",
                "action": "fetch",
                "outcome": "success",
                "count": -1,
            },
        ]
        result = build_hourly_change_digest(events)
        # Both have count <= 0 so should be skipped
        assert result == {"bucket": "hourly", "hours": {}}

    def test_missing_timestamp_raises(self) -> None:
        events = [
            {
                "connector": "github",
                "action": "fetch",
                "outcome": "success",
            }
        ]
        with pytest.raises(ValueError, match="each event must include a timestamp"):
            build_hourly_change_digest(events)

    def test_defaults_for_missing_fields(self) -> None:
        events = [
            {
                "timestamp": "2026-06-29T12:00:00Z",
            }
        ]
        result = build_hourly_change_digest(events)
        assert result["hours"]["2026-06-29T12:00:00Z"]["unknown"]["unknown:unknown"] == 1


class TestResearchStore:
    """ResearchStore CRUD operations with temporary DB."""

    def test_upsert_and_get_recent(self) -> None:
        with TemporaryDirectory() as tmp:
            store = ResearchStore(Path(tmp) / "test.db")
            item = ResearchItem(
                slug="abc123",
                source="arxiv",
                url="https://arxiv.org/abs/1234.56789",
                title="Test Paper",
                summary="A test paper abstract.",
                score=10,
                tags=["ml"],
                fetched_at=datetime.now(UTC),
                relevance=0.8,
            )
            store.upsert(item)
            recent = store.get_recent(hours=24, limit=10)
            assert len(recent) == 1
            assert recent[0].slug == "abc123"
            assert recent[0].title == "Test Paper"

    def test_search(self) -> None:
        with TemporaryDirectory() as tmp:
            store = ResearchStore(Path(tmp) / "test.db")
            store.upsert(
                ResearchItem(
                    slug="s1",
                    source="github",
                    url="https://github.com/a/b",
                    title="Awesome Project",
                    summary="An awesome project for testing",
                    fetched_at=datetime.now(UTC),
                )
            )
            store.upsert(
                ResearchItem(
                    slug="s2",
                    source="hn",
                    url="https://news.ycombinator.com/item?id=1",
                    title="Unrelated Post",
                    summary="Something else entirely",
                    fetched_at=datetime.now(UTC),
                )
            )
            results = store.search("awesome", limit=10)
            assert len(results) == 1
            assert results[0].slug == "s1"

    def test_search_no_results(self) -> None:
        with TemporaryDirectory() as tmp:
            store = ResearchStore(Path(tmp) / "test.db")
            store.upsert(
                ResearchItem(
                    slug="s1",
                    source="hn",
                    url="https://example.com",
                    title="Title",
                    summary="Summary",
                    fetched_at=datetime.now(UTC),
                )
            )
            results = store.search("nonexistent", limit=10)
            assert results == []


class TestSessionHook:
    """inject_session_context function."""

    def test_no_items(self) -> None:
        with TemporaryDirectory() as tmp:
            store = ResearchStore(Path(tmp) / "test.db")
            result = inject_session_context(store, hours=24, limit=5)
        assert "Recent Research" in result
        assert "No recent research items" in result

    def test_with_items(self) -> None:
        with TemporaryDirectory() as tmp:
            store = ResearchStore(Path(tmp) / "test.db")
            store.upsert(
                ResearchItem(
                    slug="hook1",
                    source="hn",
                    url="https://example.com/hook",
                    title="Hook Test",
                    summary="Testing the session hook.",
                    score=7,
                    tags=["test"],
                    fetched_at=datetime.now(UTC),
                    relevance=0.6,
                )
            )
            result = inject_session_context(store, hours=24, limit=5)
        assert "Hook Test" in result
        assert "score 7" in result


class TestTopicExtractor:
    """TopicExtractor — requires a temporary project root."""

    def test_empty_directory(self) -> None:
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            extractor = TopicExtractor(project_root=root)
            topics = extractor.extract()
        assert topics == []

    def test_with_pyproject_deps(self) -> None:
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "pyproject.toml").write_text(
                '[project]\ndependencies = [\n  "flask>=2.0",\n  "requests",\n]\n'
            )
            extractor = TopicExtractor(project_root=root)
            topics = extractor.extract()
        # Should extract package names from dep strings
        assert "flask" in topics
        assert "requests" in topics


class TestModuleImports:
    """Verify all core modules are importable."""

    def test_import_schema(self) -> None:
        from research_engine import schema as _

        assert _ is not None

    def test_import_store(self) -> None:
        from research_engine import store as _

        assert _ is not None

    def test_import_digest(self) -> None:
        from research_engine import digest as _

        assert _ is not None

    def test_import_topics(self) -> None:
        from research_engine import topics as _

        assert _ is not None

    def test_import_scheduler(self) -> None:
        from research_engine import scheduler as _

        assert _ is not None

    def test_import_session_hook(self) -> None:
        from research_engine import session_hook as _

        assert _ is not None

    def test_import_mcp_tools(self) -> None:
        from research_engine.mcp import tools as _

        assert _ is not None

    def test_import_version(self) -> None:
        from research_engine import __version__

        assert __version__ == "0.1.0"

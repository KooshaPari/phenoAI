"""Regression tests for catalogue-level validation rules."""

from scripts.validate_catalog import duplicate_ids


def test_duplicate_ids_detects_repeated_ids_within_a_section() -> None:
    """Reject repeated IDs even when their entry metadata differs."""
    catalog = {
        "servers": [
            {"id": "duplicate", "title": "First"},
            {"id": "duplicate", "title": "Second"},
        ],
        "skills": [{"id": "shared-name"}],
        "plugins": [{"id": "shared-name"}],
    }

    assert duplicate_ids(catalog) == [("servers", "duplicate")]


def test_duplicate_ids_accepts_unique_ids_in_each_namespace() -> None:
    """Allow separate namespaces to use the same stable ID."""
    catalog = {
        "servers": [{"id": "shared-name"}],
        "skills": [{"id": "shared-name"}],
        "plugins": [{"id": "shared-name"}],
        "agents": [{"id": "shared-name"}],
    }

    assert duplicate_ids(catalog) == []

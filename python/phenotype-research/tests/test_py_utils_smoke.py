"""Smoke tests for the ``phenotype-py-utils`` runtime dependency.

Verifies the 5 advertised utility functions are importable and callable.
This is a dependency-only adoption: source files are not yet refactored
to use the shared implementations (V9-T3-6 wave 2).
"""

from __future__ import annotations

import pytest

pytest.importorskip("phenotype_py_utils")

from phenotype_py_utils import (  # type: ignore[import-untyped]
    iso_now,
    load_config,
    parse_args,
    setup_logging,
    truncate,
)


def test_load_config_callable() -> None:
    assert callable(load_config)


def test_setup_logging_callable() -> None:
    assert callable(setup_logging)


def test_parse_args_callable() -> None:
    assert callable(parse_args)


def test_iso_now_callable() -> None:
    s = iso_now()
    assert isinstance(s, str)
    assert s.endswith("Z")


def test_truncate_callable() -> None:
    assert callable(truncate)
    assert truncate("phenotype-py-utils", max_len=12) == "phenotype..."
    assert truncate("hi", max_len=10) == "hi"

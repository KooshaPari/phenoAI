"""Research Intelligence Engine CLI. @trace FR-RES-040"""

from __future__ import annotations

from pathlib import Path

import typer

from research_engine.digest import DigestGenerator
from research_engine.scheduler import TieredScheduler
from research_engine.store import ResearchStore
from research_engine.topics import TopicExtractor

app = typer.Typer(name="research", help="Research Intelligence Engine commands.")

_GLOBAL_DB = Path.home() / ".thegent" / "research.db"


@app.command()
def digest(
    hours: int = typer.Option(24, help="Look-back window in hours."),
    limit: int = typer.Option(20, help="Max items."),
) -> None:
    """Print a markdown research digest."""
    store = ResearchStore(_GLOBAL_DB)
    gen = DigestGenerator(store)
    typer.echo(gen.generate(hours=hours, limit=limit))


@app.command()
def topics() -> None:
    """List detected project topics."""
    detected = TopicExtractor(project_root=Path.cwd()).extract()
    if not detected:
        typer.echo("No topics detected.")
        return
    for t in detected:
        typer.echo(f"  - {t}")


@app.command()
def crawl() -> None:
    """Trigger an immediate one-shot crawl of all sources."""
    topics = TopicExtractor(project_root=Path.cwd()).extract()
    scheduler = TieredScheduler(_GLOBAL_DB, topics)
    typer.echo("Crawling all sources…")
    scheduler._run_tier("hourly")
    scheduler._run_tier("daily")
    scheduler._run_tier("weekly")
    typer.echo("Done.")


@app.command()
def sync(
    project_db: str = typer.Argument(..., help="Path to project-local research DB."),
    min_relevance: float = typer.Option(0.5, help="Minimum relevance score."),
) -> None:
    """Sync global research DB to a project-local DB."""
    store = ResearchStore(_GLOBAL_DB)
    n = store.mirror_to_project(Path(project_db), min_relevance=min_relevance)
    typer.echo(f"Synced {n} items to {project_db}.")


@app.command()
def search(
    query: str = typer.Argument(..., help="Search query."),
    limit: int = typer.Option(20, help="Max results."),
) -> None:
    """Search research items by keyword."""
    store = ResearchStore(_GLOBAL_DB)
    items = store.search(query)[:limit]
    if not items:
        typer.echo("No results found.")
        return
    for item in items:
        typer.echo(f"[{item.source}] {item.title} — {item.url}")

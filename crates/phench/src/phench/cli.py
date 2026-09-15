from __future__ import annotations

from dataclasses import asdict

import orjson as json
import typer
from rich.console import Console
from rich.prompt import IntPrompt

from .service import (
    add_repo,
    init_target,
    list_targets,
    lock_target,
    materialize_target,
    run_env_doctor_for_target,
    run_target,
    sync_target,
    target_status,
    target_timeline,
)

console = Console()
app = typer.Typer(help="Phench: deterministic project runtime targets and execution.")
target_app = typer.Typer(help="Manage project runtime targets.")
env_app = typer.Typer(help="Environment preflight commands for targets.")
app.add_typer(target_app, name="target")
app.add_typer(env_app, name="env")


@target_app.command("init")
def target_init_cmd(name: str, mode: str = "repo") -> None:
    if mode not in {"repo", "stack"}:
        raise typer.BadParameter("mode must be one of: repo, stack")
    lock = init_target(name, mode=mode)
    console.print_json(json.dumps({"target": lock.target_name, "mode": lock.mode, "lock_hash": lock.lock_hash}).decode())


@target_app.command("add-repo")
def target_add_repo_cmd(
    name: str,
    repo: str = typer.Option(..., "--repo"),
    ref: str = typer.Option(..., "--ref"),
    repo_id: str | None = typer.Option(None, "--repo-id"),
    worktree: str | None = typer.Option(None, "--worktree"),
) -> None:
    lock = add_repo(name, repo, ref, repo_id=repo_id, worktree_path=worktree)
    console.print_json(
        json.dumps({"target": lock.target_name, "repos": [repo.repo_id for repo in lock.repos], "lock_hash": lock.lock_hash}).decode()
    )


@target_app.command("lock")
def target_lock_cmd(name: str) -> None:
    lock = lock_target(name)
    console.print_json(
        json.dumps(
            {
                "target": lock.target_name,
                "lock_hash": lock.lock_hash,
                "repos": [
                    {"repo_id": repo.repo_id, "selected_ref": repo.selected_ref, "resolved_sha": repo.resolved_sha}
                    for repo in lock.repos
                ],
            }
        ).decode()
    )


@target_app.command("materialize")
def target_materialize_cmd(name: str) -> None:
    runtime = materialize_target(name)
    console.print_json(
        json.dumps(
            {
                "target": runtime.target_name,
                "materialized_root": runtime.materialized_root,
                "repos": [asdict(repo) for repo in runtime.repo_materializations],
            }
        ).decode()
    )


@app.command("timeline")
def timeline_cmd(name: str, repo_id: str | None = None, limit: int = 30) -> None:
    console.print_json(json.dumps(target_timeline(name, repo_id=repo_id, limit=limit)).decode())


@app.command("run")
def run_cmd(
    name: str,
    repo_id: str | None = typer.Option(None, "--repo-id"),
    runner: str | None = typer.Option(None, "--runner"),
    command: str | None = typer.Option(None, "--command"),
    all_repos: bool = typer.Option(False, "--all-repos"),
    execution_mode: str = typer.Option("serial", "--mode"),
) -> None:
    code = run_target(
        name,
        repo_id=repo_id,
        runner=runner,
        command_name=command,
        all_repos=all_repos,
        execution_mode=execution_mode,
    )
    raise typer.Exit(code)


@env_app.command("doctor")
def env_doctor_cmd(name: str) -> None:
    report = run_env_doctor_for_target(name)
    console.print_json(json.dumps(report).decode())
    if report["doctor_status"] != "pass":
        raise typer.Exit(2)


@app.command("sync")
def sync_cmd(name: str, prefer: str | None = None) -> None:
    console.print_json(json.dumps(sync_target(name, prefer=prefer)).decode())


@app.command("status")
def status_cmd(name: str) -> None:
    console.print_json(json.dumps(target_status(name)).decode())


@app.command("tui")
def tui_cmd() -> None:
    targets = list_targets()
    if not targets:
        raise typer.BadParameter("No targets found under Phenotype/projects.")

    console.print("Select target:")
    for idx, target in enumerate(targets, start=1):
        console.print(f"{idx}. {target}")
    target_index = IntPrompt.ask("Target number", default=1)
    if target_index < 1 or target_index > len(targets):
        raise typer.BadParameter("Target selection out of range.")
    selected_target = targets[target_index - 1]

    timeline = target_timeline(selected_target, limit=20)
    console.print(f"Timeline for [bold]{selected_target}[/bold] ({timeline['repo_id']}):")
    for line in timeline.get("recent", []):
        console.print(f"  {line}")

    code = run_target(selected_target)
    raise typer.Exit(code)


def main() -> None:
    app()


if __name__ == "__main__":
    main()

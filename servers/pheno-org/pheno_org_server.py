"""Pheno-org MCP server.

Native PhenoMCPServers FastMCP server for the six org tool groups
(governance, agent, knowledge, policy, session, workflow). The surface was
originally migrated from ``PhenoMCP/python/src/pheno_mcp/tools``. All tools
proxy to a Parpoura HTTP backend via ``httpx.AsyncClient``.

Env:
  PARPOURA_BASE_URL  -- base URL for the Parpoura API (default http://localhost:8001)
"""
from __future__ import annotations

import os
from typing import Any
from urllib.parse import quote

import httpx
from fastmcp import FastMCP

mcp = FastMCP("pheno-org")

_BASE_URL = os.environ.get("PARPOURA_BASE_URL", "http://localhost:8001")
_TIMEOUT = 30.0


def _client() -> httpx.AsyncClient:
    return httpx.AsyncClient(base_url=_BASE_URL, timeout=_TIMEOUT)


# ---------------------------------------------------------------------------
# Governance / ledger
# ---------------------------------------------------------------------------


@mcp.tool()
async def ledger_query(
    from_entry: str | None = None,
    to_entry: str | None = None,
    action: str | None = None,
    actor: str | None = None,
    workflow_id: str | None = None,
    limit: int = 100,
) -> dict[str, Any]:
    """Query the governance ledger with optional filters.

    Returns entries with checksums and hash-chain metadata.
    """
    params: dict[str, Any] = {}
    for key in ("from_entry", "to_entry", "action", "actor", "workflow_id", "limit"):
        value = locals()[key]
        if value is not None:
            params[key] = value
    async with _client() as client:
        try:
            response = await client.get("/api/v1/governance/ledger", params=params)
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


@mcp.tool()
async def ledger_verify(from_entry: str, to_entry: str) -> dict[str, Any]:
    """Verify the integrity of the ledger hash chain between two entry IDs."""
    async with _client() as client:
        try:
            response = await client.post(
                "/api/v1/governance/ledger/verify",
                json={"from_entry": from_entry, "to_entry": to_entry},
            )
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------


@mcp.tool()
async def agent_create(
    name: str,
    agent_id: str | None = None,
    description: str | None = None,
) -> dict[str, Any]:
    """Create a new agent record in Parpoura."""
    payload: dict[str, Any] = {"name": name}
    if agent_id is not None:
        payload["agent_id"] = agent_id
    if description is not None:
        payload["description"] = description
    async with _client() as client:
        try:
            response = await client.post("/api/v1/agents", json=payload)
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


@mcp.tool()
async def agent_list() -> dict[str, Any]:
    """List registered agents."""
    async with _client() as client:
        try:
            response = await client.get("/api/v1/agents")
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


@mcp.tool()
async def agent_get(agent_id: str) -> dict[str, Any]:
    """Fetch a single agent by identifier."""
    async with _client() as client:
        try:
            response = await client.get(f"/api/v1/agents/{quote(agent_id, safe='')}")
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


@mcp.tool()
async def agent_delete(agent_id: str) -> dict[str, Any]:
    """Delete an agent by identifier."""
    async with _client() as client:
        try:
            response = await client.delete(f"/api/v1/agents/{quote(agent_id, safe='')}")
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


# ---------------------------------------------------------------------------
# Knowledge
# ---------------------------------------------------------------------------


@mcp.tool()
async def knowledge_store(
    knowledge_id: str,
    content: str,
    metadata: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Store a knowledge item in Parpoura."""
    payload: dict[str, Any] = {"knowledge_id": knowledge_id, "content": content}
    if metadata is not None:
        payload["metadata"] = metadata
    async with _client() as client:
        try:
            response = await client.post("/api/v1/knowledge", json=payload)
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


@mcp.tool()
async def knowledge_retrieve(knowledge_id: str) -> dict[str, Any]:
    """Retrieve a knowledge item by identifier."""
    async with _client() as client:
        try:
            response = await client.get(f"/api/v1/knowledge/{quote(knowledge_id, safe='')}")
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


@mcp.tool()
async def knowledge_search(query: str, limit: int = 20) -> dict[str, Any]:
    """Search knowledge items."""
    params: dict[str, Any] = {"query": query, "limit": limit}
    async with _client() as client:
        try:
            response = await client.get("/api/v1/knowledge/search", params=params)
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


@mcp.tool()
async def knowledge_delete(knowledge_id: str) -> dict[str, Any]:
    """Delete a knowledge item by identifier."""
    async with _client() as client:
        try:
            response = await client.delete(f"/api/v1/knowledge/{quote(knowledge_id, safe='')}")
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


# ---------------------------------------------------------------------------
# Policy
# ---------------------------------------------------------------------------


@mcp.tool()
async def policy_list() -> dict[str, Any]:
    """List available policies."""
    async with _client() as client:
        try:
            response = await client.get("/api/v1/policies")
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


@mcp.tool()
async def policy_get(policy_id: str) -> dict[str, Any]:
    """Fetch a policy by identifier."""
    async with _client() as client:
        try:
            response = await client.get(f"/api/v1/policies/{quote(policy_id, safe='')}")
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


@mcp.tool()
async def policy_evaluate(policy_id: str, context: dict[str, Any]) -> dict[str, Any]:
    """Evaluate a policy against the supplied context."""
    async with _client() as client:
        try:
            response = await client.post(
                "/api/v1/policies/evaluate",
                json={"policy_id": policy_id, "context": context},
            )
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


# ---------------------------------------------------------------------------
# Session
# ---------------------------------------------------------------------------


@mcp.tool()
async def session_suspend(session_id: str) -> dict[str, Any]:
    """Suspend a running session, producing a serialised session bundle."""
    async with _client() as client:
        try:
            response = await client.post(f"/api/v1/sessions/{session_id}/suspend")
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


@mcp.tool()
async def session_resume(bundle_ref: str) -> dict[str, Any]:
    """Resume a previously suspended session from its bundle reference."""
    async with _client() as client:
        try:
            response = await client.post(
                "/api/v1/sessions/resume",
                json={"bundle_ref": bundle_ref},
            )
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


# ---------------------------------------------------------------------------
# Workflow
# ---------------------------------------------------------------------------


@mcp.tool()
async def workflow_execute(workflow_id: str, workflow_type: str = "default") -> dict[str, Any]:
    """Trigger execution of a workflow in PENDING or SUSPENDED status."""
    async with _client() as client:
        try:
            response = await client.post(
                f"/workflows/{workflow_id}/execute",
                params={"workflow_type": workflow_type},
            )
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


@mcp.tool()
async def workflow_status(workflow_id: str) -> dict[str, Any]:
    """Get the current status and details of a workflow."""
    async with _client() as client:
        try:
            response = await client.get(f"/workflows/{workflow_id}")
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


@mcp.tool()
async def workflow_cancel(workflow_id: str) -> dict[str, Any]:
    """Cancel a running or pending workflow."""
    async with _client() as client:
        try:
            response = await client.post(f"/workflows/{workflow_id}/cancel")
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


@mcp.tool()
async def workflow_list() -> dict[str, Any]:
    """List all workflows with their current status."""
    async with _client() as client:
        try:
            response = await client.get("/workflows")
            response.raise_for_status()
            return response.json()
        except httpx.HTTPStatusError as exc:
            return {"error": exc.response.text, "status_code": exc.response.status_code}


if __name__ == "__main__":
    mcp.run()

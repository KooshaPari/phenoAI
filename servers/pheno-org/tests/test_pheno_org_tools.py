"""pytest: pheno-org MCP tools call Parpoura HTTP with correct shape."""
from __future__ import annotations

import asyncio
import os
import sys
from contextlib import asynccontextmanager
from unittest.mock import MagicMock, patch

import httpx
import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import pheno_org_server


@pytest.fixture(autouse=True)
def _env(monkeypatch):
    monkeypatch.setenv("PARPOURA_BASE_URL", "http://127.0.0.1:9999")


def _ok(payload: dict) -> httpx.Response:
    return httpx.Response(200, json=payload, request=httpx.Request("GET", "http://x"))


def _err(status: int, text: str) -> httpx.Response:
    return httpx.Response(status, text=text, request=httpx.Request("GET", "http://x"))


def _async_return(value):
    """Wrap a value as an awaitable."""
    async def _coro():
        return value
    return _coro()


def _mock_client() -> MagicMock:
    client = MagicMock()
    return client


def _patch_client(client: MagicMock):
    """Patch pheno_org_server._client to return a context manager wrapping `client`."""
    @asynccontextmanager
    async def _factory():
        yield client

    return patch.object(pheno_org_server, "_client", _factory)


def _run(coro):
    return asyncio.run(coro)


# ---------------------------------------------------------------------------
# governance
# ---------------------------------------------------------------------------


def test_ledger_query_passes_filters():
    client = MagicMock()
    client.get.return_value = _async_return(_ok({"entries": []}))
    with _patch_client(client):
        result = _run(pheno_org_server.ledger_query(
            action="money.ledger_entry.created.v1", actor="agent-1", limit=5
        ))
    assert result == {"entries": []}
    args, kwargs = client.get.call_args
    assert args[0] == "/api/v1/governance/ledger"
    assert kwargs["params"]["action"] == "money.ledger_entry.created.v1"
    assert kwargs["params"]["actor"] == "agent-1"
    assert kwargs["params"]["limit"] == 5


def test_ledger_verify_posts_range():
    client = MagicMock()
    client.post.return_value = _async_return(_ok({"valid": True}))
    with _patch_client(client):
        result = _run(pheno_org_server.ledger_verify("e1", "e2"))
    assert result == {"valid": True}
    args, kwargs = client.post.call_args
    assert args[0] == "/api/v1/governance/ledger/verify"
    assert kwargs["json"] == {"from_entry": "e1", "to_entry": "e2"}


# ---------------------------------------------------------------------------
# agent
# ---------------------------------------------------------------------------


def test_agent_create_posts_payload():
    client = MagicMock()
    client.post.return_value = _async_return(_ok({"id": "a1"}))
    with _patch_client(client):
        result = _run(pheno_org_server.agent_create(
            name="bot", agent_id="a1", description="test"
        ))
    assert result == {"id": "a1"}
    args, kwargs = client.post.call_args
    assert args[0] == "/api/v1/agents"
    assert kwargs["json"] == {"name": "bot", "agent_id": "a1", "description": "test"}


def test_agent_list_gets_agents():
    client = MagicMock()
    client.get.return_value = _async_return(_ok({"agents": []}))
    with _patch_client(client):
        result = _run(pheno_org_server.agent_list())
    assert result == {"agents": []}
    client.get.assert_called_with("/api/v1/agents")


def test_agent_get_url_encodes_id():
    client = MagicMock()
    client.get.return_value = _async_return(_ok({"id": "a/b"}))
    with _patch_client(client):
        _run(pheno_org_server.agent_get("a/b"))
    args, _ = client.get.call_args
    assert args[0] == "/api/v1/agents/a%2Fb"


def test_agent_delete_url_encodes_id():
    client = MagicMock()
    client.delete.return_value = _async_return(_ok({"ok": True}))
    with _patch_client(client):
        result = _run(pheno_org_server.agent_delete("a b"))
    assert result == {"ok": True}
    args, _ = client.delete.call_args
    assert args[0] == "/api/v1/agents/a%20b"


# ---------------------------------------------------------------------------
# knowledge
# ---------------------------------------------------------------------------


def test_knowledge_store_posts_payload():
    client = MagicMock()
    client.post.return_value = _async_return(_ok({"id": "k1"}))
    with _patch_client(client):
        result = _run(pheno_org_server.knowledge_store(
            knowledge_id="k1", content="hello", metadata={"src": "x"}
        ))
    assert result == {"id": "k1"}
    args, kwargs = client.post.call_args
    assert args[0] == "/api/v1/knowledge"
    assert kwargs["json"]["content"] == "hello"
    assert kwargs["json"]["metadata"] == {"src": "x"}


def test_knowledge_search_uses_query_params():
    client = MagicMock()
    client.get.return_value = _async_return(_ok({"hits": []}))
    with _patch_client(client):
        _run(pheno_org_server.knowledge_search("neo", limit=10))
    args, kwargs = client.get.call_args
    assert args[0] == "/api/v1/knowledge/search"
    assert kwargs["params"] == {"query": "neo", "limit": 10}


# ---------------------------------------------------------------------------
# policy
# ---------------------------------------------------------------------------


def test_policy_evaluate_posts_context():
    client = MagicMock()
    client.post.return_value = _async_return(_ok({"allow": True}))
    with _patch_client(client):
        result = _run(pheno_org_server.policy_evaluate(
            policy_id="p1", context={"user": "u"}
        ))
    assert result == {"allow": True}
    args, kwargs = client.post.call_args
    assert args[0] == "/api/v1/policies/evaluate"
    assert kwargs["json"] == {"policy_id": "p1", "context": {"user": "u"}}


# ---------------------------------------------------------------------------
# session
# ---------------------------------------------------------------------------


def test_session_suspend_posts_session_id():
    client = MagicMock()
    client.post.return_value = _async_return(_ok({"bundle_ref": "b1"}))
    with _patch_client(client):
        result = _run(pheno_org_server.session_suspend("s1"))
    assert result == {"bundle_ref": "b1"}
    args, _ = client.post.call_args
    assert args[0] == "/api/v1/sessions/s1/suspend"


def test_session_resume_posts_bundle_ref():
    client = MagicMock()
    client.post.return_value = _async_return(_ok({"session_id": "s2"}))
    with _patch_client(client):
        result = _run(pheno_org_server.session_resume("b1"))
    assert result == {"session_id": "s2"}
    args, kwargs = client.post.call_args
    assert args[0] == "/api/v1/sessions/resume"
    assert kwargs["json"] == {"bundle_ref": "b1"}


# ---------------------------------------------------------------------------
# workflow
# ---------------------------------------------------------------------------


def test_workflow_execute_posts_with_type():
    client = MagicMock()
    client.post.return_value = _async_return(_ok({"execution_id": "e1"}))
    with _patch_client(client):
        result = _run(pheno_org_server.workflow_execute("w1", "fast"))
    assert result == {"execution_id": "e1"}
    args, kwargs = client.post.call_args
    assert args[0] == "/workflows/w1/execute"
    assert kwargs["params"] == {"workflow_type": "fast"}


def test_workflow_status_gets_workflow():
    client = MagicMock()
    client.get.return_value = _async_return(_ok({"status": "RUNNING"}))
    with _patch_client(client):
        result = _run(pheno_org_server.workflow_status("w1"))
    assert result == {"status": "RUNNING"}
    args, _ = client.get.call_args
    assert args[0] == "/workflows/w1"


def test_workflow_list_gets_workflows():
    client = MagicMock()
    client.get.return_value = _async_return(_ok({"workflows": []}))
    with _patch_client(client):
        result = _run(pheno_org_server.workflow_list())
    assert result == {"workflows": []}
    client.get.assert_called_with("/workflows")


# ---------------------------------------------------------------------------
# error handling
# ---------------------------------------------------------------------------


def test_http_error_returns_error_payload():
    client = MagicMock()
    client.get.return_value = _async_return(_err(404, "not found"))
    with _patch_client(client):
        result = _run(pheno_org_server.agent_list())
    assert result == {"error": "not found", "status_code": 404}

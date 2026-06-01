"""Tests for onboarding-agent ADR-086 permission proposal logic.

Validates ``_persist_proposed_permission_rules`` (renamed from
``_propose_permission_rules`` in plan v2) :

- skips when the agent has already created rules (idempotence) ;
- writes the right matrix of proposals to ``onboarding.proposed_rules``
  given the user profile keys ;
- never raises even if the tools API misbehaves ;
- works without a ``ctx.tools`` (best-effort idempotence skipped).

The agent file uses hyphens in its directory name, so we load it via
``importlib.util.spec_from_file_location``.
"""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

import pytest


# ---------------------------------------------------------------------------
# Agent import
# ---------------------------------------------------------------------------

_PROJECT_ROOT = Path(__file__).resolve().parent.parent
_AGENT_PATH = _PROJECT_ROOT / "agents" / "system" / "onboarding-agent" / "agent.py"

_spec = importlib.util.spec_from_file_location("onboarding_agent_v22", str(_AGENT_PATH))
if _spec is None or _spec.loader is None:
    pytest.skip(
        f"Agent file not found: {_AGENT_PATH}",
        allow_module_level=True,
    )

_mod = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = _mod
_spec.loader.exec_module(_mod)

_persist_proposed_permission_rules = _mod._persist_proposed_permission_rules
ONBOARDING_AGENT_CREATOR = _mod.ONBOARDING_AGENT_CREATOR


# ---------------------------------------------------------------------------
# Test doubles
# ---------------------------------------------------------------------------


class _FakeMemory:
    """Captures recall() reads and remember() writes."""

    def __init__(self, store: dict[str, str] | None = None) -> None:
        self._store = store or {}
        self.writes: dict[str, str] = {}

    async def recall(self, key: str) -> str | None:
        # Reads see both the seed values AND any value previously written
        # in this test, so that the agent's own writes during finalize are
        # observable from subsequent recall() calls.
        if key in self.writes:
            return self.writes[key]
        return self._store.get(key)

    async def remember(self, *, key: str, value: str, source: str, confidence: float) -> None:
        self.writes[key] = value

    async def remember_user(self, *, key: str, value: str, source: str, confidence: float) -> None:
        self.writes[key] = value


class _FakeTools:
    """Records every call(); returns canned responses by tool name."""

    def __init__(
        self,
        list_response: dict[str, Any] | None = None,
        list_should_raise: bool = False,
    ) -> None:
        self.calls: list[tuple[str, dict[str, Any]]] = []
        self._list_response = list_response or {"rules": []}
        self._list_should_raise = list_should_raise

    async def call(self, tool_name: str, payload: dict[str, Any]) -> dict[str, Any]:
        self.calls.append((tool_name, payload))
        if tool_name == "permission_rule_list":
            if self._list_should_raise:
                raise RuntimeError("dispatcher unavailable")
            return self._list_response
        # The new flow does NOT call permission_rule_add - surfacing it
        # in a test would be a regression.
        raise AssertionError(f"unexpected tool call: {tool_name}")


class _FakeCtx:
    """Minimal ctx surface used by ``_persist_proposed_permission_rules``."""

    def __init__(
        self,
        memory: _FakeMemory,
        tools: _FakeTools | None,
    ) -> None:
        self.memory = memory
        self.tools = tools


def _proposals_from(memory: _FakeMemory) -> list[dict[str, Any]]:
    raw = memory.writes.get("onboarding.proposed_rules")
    if raw is None:
        return []
    return json.loads(raw)


# ---------------------------------------------------------------------------
# Tests - idempotence + degraded paths
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_writes_proposals_even_when_governance_has_prior_rules() -> None:
    # GIVEN sovereignty=local-only ET des règles déjà persistées en gov.db
    # par une session précédente d'onboarding (cas réel : utilisateur qui
    # reset puis re-onboard).
    memory = _FakeMemory({"user.constraints.sovereignty": "local-only"})
    tools = _FakeTools(
        list_response={"rules": [{"id": 42, "tool_name": "http_fetch"}]},
    )
    ctx = _FakeCtx(memory, tools)

    # WHEN
    await _persist_proposed_permission_rules(ctx)

    # THEN les propositions sont (re)écrites en mémoire - pas d'idempotence
    # côté agent, le desktop dédupe lors de l'apply si nécessaire.
    proposals = _proposals_from(memory)
    assert proposals, "expected fresh proposals despite prior gov.db rules"


@pytest.mark.asyncio
async def test_continues_when_permission_rule_list_fails() -> None:
    # GIVEN un dispatcher indisponible
    memory = _FakeMemory({"user.constraints.sovereignty": "local-only"})
    tools = _FakeTools(list_should_raise=True)
    ctx = _FakeCtx(memory, tools)

    await _persist_proposed_permission_rules(ctx)

    # On a quand même écrit les propositions - la défaillance de l'historique
    # ne doit pas bloquer la dérivation.
    proposals = _proposals_from(memory)
    assert proposals, "expected proposals despite list failure"


@pytest.mark.asyncio
async def test_runs_without_tools_attribute() -> None:
    # GIVEN un ctx sans dispatcher (ex. environnement de test minimal)
    memory = _FakeMemory({"user.constraints.sovereignty": "local-only"})
    ctx = _FakeCtx(memory, tools=None)

    await _persist_proposed_permission_rules(ctx)
    proposals = _proposals_from(memory)
    assert proposals, "should still emit proposals without tools"


# ---------------------------------------------------------------------------
# Tests - matrix coverage (sovereignty × hitl × integrations)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_local_only_emits_two_https_http_deny() -> None:
    memory = _FakeMemory({"user.constraints.sovereignty": "local-only"})
    ctx = _FakeCtx(memory, tools=_FakeTools())

    await _persist_proposed_permission_rules(ctx)

    proposals = _proposals_from(memory)
    deny_prefixes = {
        p["arg_prefix"]
        for p in proposals
        if p["tool_name"] == "http_fetch" and p["action"] == "deny"
    }
    assert "https://" in deny_prefixes, proposals
    assert "http://" in deny_prefixes, proposals


@pytest.mark.asyncio
async def test_local_preferred_denies_cloud_llm_endpoints() -> None:
    memory = _FakeMemory({"user.constraints.sovereignty": "local-preferred"})
    ctx = _FakeCtx(memory, tools=_FakeTools())

    await _persist_proposed_permission_rules(ctx)

    proposals = _proposals_from(memory)
    deny_prefixes = {
        p["arg_prefix"]
        for p in proposals
        if p["tool_name"] == "http_fetch" and p["action"] == "deny"
    }
    assert "https://api.openai.com" in deny_prefixes, proposals
    assert "https://api.anthropic.com" in deny_prefixes, proposals


@pytest.mark.asyncio
async def test_cloud_ok_no_network_rule() -> None:
    memory = _FakeMemory({"user.constraints.sovereignty": "cloud-ok"})
    ctx = _FakeCtx(memory, tools=_FakeTools())

    await _persist_proposed_permission_rules(ctx)

    proposals = _proposals_from(memory)
    network_rules = [p for p in proposals if p["tool_name"] == "http_fetch"]
    assert network_rules == [], proposals


@pytest.mark.asyncio
async def test_hitl_critical_only_allows_read_safe_tools() -> None:
    memory = _FakeMemory({
        "user.constraints.sovereignty": "cloud-ok",
        "user.agents.hitl": "critical-only",
    })
    ctx = _FakeCtx(memory, tools=_FakeTools())

    await _persist_proposed_permission_rules(ctx)

    proposals = _proposals_from(memory)
    assert any(
        p["tool_name"] == "file_read" and p["action"] == "allow"
        for p in proposals
    ), proposals
    shell_allows = {
        p.get("arg_prefix")
        for p in proposals
        if p["tool_name"] == "shell_exec" and p["action"] == "allow"
    }
    assert {"ls", "cat", "grep"}.issubset(shell_allows), shell_allows


@pytest.mark.asyncio
async def test_hitl_never_also_allows_read_safe_tools() -> None:
    memory = _FakeMemory({
        "user.constraints.sovereignty": "cloud-ok",
        "user.agents.hitl": "never",
    })
    ctx = _FakeCtx(memory, tools=_FakeTools())

    await _persist_proposed_permission_rules(ctx)

    proposals = _proposals_from(memory)
    assert any(
        p["tool_name"] == "file_read" and p["action"] == "allow"
        for p in proposals
    ), proposals


@pytest.mark.asyncio
async def test_hitl_always_emits_no_allow_rule() -> None:
    memory = _FakeMemory({
        "user.constraints.sovereignty": "cloud-ok",
        "user.agents.hitl": "always",
    })
    ctx = _FakeCtx(memory, tools=_FakeTools())

    await _persist_proposed_permission_rules(ctx)

    proposals = _proposals_from(memory)
    assert all(p["action"] != "allow" for p in proposals), proposals


@pytest.mark.asyncio
async def test_github_integration_allows_api_endpoint() -> None:
    memory = _FakeMemory({
        "user.constraints.sovereignty": "local-preferred",
        "user.tools.integrations": "GitHub, Notion",
    })
    ctx = _FakeCtx(memory, tools=_FakeTools())

    await _persist_proposed_permission_rules(ctx)

    proposals = _proposals_from(memory)
    github_allows = [
        p
        for p in proposals
        if p["tool_name"] == "http_fetch"
        and p["action"] == "allow"
        and p.get("arg_prefix", "").startswith("https://api.github.com")
    ]
    notion_allows = [
        p
        for p in proposals
        if p["tool_name"] == "http_fetch"
        and p["action"] == "allow"
        and p.get("arg_prefix", "").startswith("https://api.notion.com")
    ]
    assert github_allows, proposals
    assert notion_allows, proposals


@pytest.mark.asyncio
async def test_no_proposals_writes_empty_array() -> None:
    # cloud-ok + hitl=always + no integrations → matrice vide → on écrit "[]"
    # pour effacer toute liste résiduelle d'une session précédente.
    memory = _FakeMemory({
        "user.constraints.sovereignty": "cloud-ok",
        "user.agents.hitl": "always",
    })
    ctx = _FakeCtx(memory, tools=_FakeTools())

    await _persist_proposed_permission_rules(ctx)

    raw = memory.writes.get("onboarding.proposed_rules")
    assert raw == "[]", raw

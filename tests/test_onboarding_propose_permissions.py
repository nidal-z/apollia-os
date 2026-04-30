"""Tests for onboarding-agent ADR-086 permission proposal logic.

Validates that ``_propose_permission_rules`` :

- skips when the agent has already created rules (idempotence) ;
- proposes a deny rule on `http_fetch` when sovereignty is `local-only` ;
- never raises even if the tools API misbehaves.

The agent file uses hyphens in its directory name, so we load it via
``importlib.util.spec_from_file_location``.
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path
from typing import Any

import pytest


# ---------------------------------------------------------------------------
# Agent import
# ---------------------------------------------------------------------------

_PROJECT_ROOT = Path(__file__).resolve().parent.parent
_AGENT_PATH = _PROJECT_ROOT / "agents" / "system" / "onboarding-agent" / "agent.py"

_spec = importlib.util.spec_from_file_location("onboarding_agent_v21", str(_AGENT_PATH))
if _spec is None or _spec.loader is None:
    pytest.skip(
        f"Agent file not found: {_AGENT_PATH}",
        allow_module_level=True,
    )

_mod = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = _mod
_spec.loader.exec_module(_mod)

_propose_permission_rules = _mod._propose_permission_rules
ONBOARDING_AGENT_CREATOR = _mod.ONBOARDING_AGENT_CREATOR


# ---------------------------------------------------------------------------
# Test doubles
# ---------------------------------------------------------------------------


class _FakeMemory:
    """Captures recall() calls and returns canned values."""

    def __init__(self, store: dict[str, str] | None = None) -> None:
        self._store = store or {}

    async def recall(self, key: str) -> str | None:
        return self._store.get(key)


class _FakeTools:
    """Records every call() invocation; returns canned responses by tool name."""

    def __init__(
        self,
        list_response: dict[str, Any] | None = None,
        add_should_raise: bool = False,
    ) -> None:
        self.calls: list[tuple[str, dict[str, Any]]] = []
        self._list_response = list_response or {"rules": []}
        self._add_should_raise = add_should_raise

    async def call(self, tool_name: str, payload: dict[str, Any]) -> dict[str, Any]:
        self.calls.append((tool_name, payload))
        if tool_name == "permission_rule_list":
            return self._list_response
        if tool_name == "permission_rule_add":
            if self._add_should_raise:
                raise RuntimeError("HITL refused")
            return {"rule_id": len(self.calls)}
        raise AssertionError(f"unexpected tool call: {tool_name}")


class _FakeCtx:
    """Minimal ctx surface used by ``_propose_permission_rules``."""

    def __init__(
        self,
        memory: _FakeMemory,
        tools: _FakeTools | None,
    ) -> None:
        self.memory = memory
        self.tools = tools


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_propose_skips_when_existing_onboarding_rules() -> None:
    # GIVEN une mémoire avec sovereignty=local-only ET des règles déjà créées
    # par l'onboarding-agent
    memory = _FakeMemory({"user.constraints.sovereignty": "local-only"})
    tools = _FakeTools(
        list_response={"rules": [{"id": 42, "tool_name": "http_fetch"}]},
    )
    ctx = _FakeCtx(memory, tools)

    # WHEN _propose_permission_rules est appelé
    await _propose_permission_rules(ctx)

    # THEN seul l'appel list a été fait — aucun add (idempotence)
    assert tools.calls == [
        (
            "permission_rule_list",
            {"created_by": ONBOARDING_AGENT_CREATOR},
        )
    ]


@pytest.mark.asyncio
async def test_propose_emits_deny_for_local_only_sovereignty() -> None:
    # GIVEN sovereignty=local-only sans règles existantes
    memory = _FakeMemory({"user.constraints.sovereignty": "local-only"})
    tools = _FakeTools(list_response={"rules": []})
    ctx = _FakeCtx(memory, tools)

    # WHEN
    await _propose_permission_rules(ctx)

    # THEN au moins un permission_rule_add ciblant http_fetch + https://
    add_calls = [c for c in tools.calls if c[0] == "permission_rule_add"]
    assert add_calls, "expected at least one permission_rule_add call"
    https_deny = [
        c
        for _, c in add_calls
        if c["tool_name"] == "http_fetch"
        and c["action"] == "deny"
        and c.get("arg_prefix") == "https://"
    ]
    assert https_deny, f"expected deny https:// rule, got: {add_calls}"


@pytest.mark.asyncio
async def test_propose_emits_nothing_when_sovereignty_is_cloud_ok() -> None:
    # GIVEN sovereignty=cloud-ok
    memory = _FakeMemory({"user.constraints.sovereignty": "cloud-ok"})
    tools = _FakeTools(list_response={"rules": []})
    ctx = _FakeCtx(memory, tools)

    # WHEN
    await _propose_permission_rules(ctx)

    # THEN aucun permission_rule_add (l'utilisateur autorise tout)
    add_calls = [c for c in tools.calls if c[0] == "permission_rule_add"]
    assert add_calls == []


@pytest.mark.asyncio
async def test_propose_swallows_add_exceptions() -> None:
    # GIVEN sovereignty=local-only ET un outil qui refuse via HITL
    memory = _FakeMemory({"user.constraints.sovereignty": "local-only"})
    tools = _FakeTools(list_response={"rules": []}, add_should_raise=True)
    ctx = _FakeCtx(memory, tools)

    # WHEN / THEN ne doit jamais propager
    await _propose_permission_rules(ctx)


@pytest.mark.asyncio
async def test_propose_no_op_when_tools_unavailable() -> None:
    # GIVEN un ctx sans attribut tools (cas runtime sans dispatcher)
    memory = _FakeMemory({"user.constraints.sovereignty": "local-only"})
    ctx = _FakeCtx(memory, tools=None)

    # WHEN / THEN
    await _propose_permission_rules(ctx)

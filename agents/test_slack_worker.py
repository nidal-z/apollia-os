"""Unit tests for slack-worker agent."""

from __future__ import annotations

import importlib.util
import pathlib
import sys
from typing import Any
from unittest.mock import AsyncMock, patch

import pytest

_spec = importlib.util.spec_from_file_location(
    "slack_worker",
    pathlib.Path(__file__).parent / "slack-worker.py",
)
assert _spec is not None and _spec.loader is not None
_module = importlib.util.module_from_spec(_spec)
sys.modules["slack_worker"] = _module
_spec.loader.exec_module(_module)  # type: ignore[union-attr]

from slack_worker import _SLACK_TOKEN_VAR, agent, manifest  # noqa: E402


def test_manifest_contains_required_fields() -> None:
    # GIVEN slack-worker is imported
    # WHEN manifest() is called
    m = manifest()
    # THEN it contains all required AIP fields
    assert m["name"] == "slack-worker"
    assert m["version"] == "0.1.0"
    assert isinstance(m["description"], str) and m["description"]
    assert m["supports_a2a"] is True


def test_manifest_declares_send_message_hitl() -> None:
    # GIVEN slack-worker manifest
    # WHEN tools_requiring_approval is inspected
    m = manifest()
    # THEN send-message is listed as requiring HITL approval
    assert "send-message" in m["tools_requiring_approval"]


def test_manifest_declares_expected_skills() -> None:
    # GIVEN slack-worker manifest
    # WHEN skills are inspected
    m = manifest()
    skill_ids = [s["id"] for s in m["skills"]]
    # THEN both send-message and read-channel are declared
    assert "send-message" in skill_ids
    assert "read-channel" in skill_ids


def test_manifest_uses_http_fetch_tool() -> None:
    # GIVEN slack-worker manifest
    # WHEN tools_required is inspected
    m = manifest()
    # THEN http_fetch is listed (not python_executor)
    assert "http_fetch" in m["tools_required"]
    assert "python_executor" not in m["tools_required"]


def test_manifest_skills_have_descriptions() -> None:
    # GIVEN slack-worker manifest
    # WHEN each skill is inspected
    m = manifest()
    # THEN every skill has a non-empty name and description
    for skill in m["skills"]:
        assert skill.get("name"), f"Skill {skill.get('id')} missing name"
        assert skill.get("description"), f"Skill {skill.get('id')} missing description"


def test_manifest_no_packages_required() -> None:
    # GIVEN slack-worker manifest
    # WHEN packages is inspected
    m = manifest()
    # THEN no third-party packages are required (uses http_fetch tool)
    assert m.get("packages", []) == []


@pytest.mark.asyncio
async def test_run_fails_with_explicit_message_when_token_missing(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    # GIVEN APOLLIA_SLACK_TOKEN is not configured
    monkeypatch.delenv(_SLACK_TOKEN_VAR, raising=False)
    # WHEN run() is called
    task: dict[str, Any] = {"input": {"text": "Send hello to #general"}}
    result = await agent.run(task, ctx=None)
    # THEN run() returns a failure result with MISSING_CONFIG code and the var name
    assert result.get("status") == "failed"
    assert "MISSING_CONFIG" in str(result)
    assert _SLACK_TOKEN_VAR in str(result)


@pytest.mark.asyncio
async def test_run_delegates_to_react_when_token_present(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    # GIVEN APOLLIA_SLACK_TOKEN is configured
    monkeypatch.setenv(_SLACK_TOKEN_VAR, "xoxb-test-token")
    # WHEN run() is called (react() is mocked to avoid actual LLM calls)
    task: dict[str, Any] = {"input": {"text": "Send hello to #general"}}
    with patch.object(type(agent), "react", new=AsyncMock(return_value="ok")) as mock_react:
        result = await agent.run(task, ctx=object())
        # THEN react() is called (config check passed)
        mock_react.assert_awaited_once()
    assert result.get("status") == "completed"


def test_agent_instance_has_correct_manifest_name() -> None:
    # GIVEN the module-level agent instance
    # WHEN manifest() is called on the instance
    m: dict[str, Any] = agent.manifest()
    # THEN the name matches the expected worker name
    assert m["name"] == "slack-worker"

"""Unit tests for email-worker agent."""

from __future__ import annotations

import importlib.util
import os
import pathlib
import sys
from typing import Any
from unittest.mock import AsyncMock, patch

import pytest

_spec = importlib.util.spec_from_file_location(
    "email_worker",
    pathlib.Path(__file__).parent / "email-worker.py",
)
assert _spec is not None and _spec.loader is not None
_module = importlib.util.module_from_spec(_spec)
sys.modules["email_worker"] = _module
_spec.loader.exec_module(_module)  # type: ignore[union-attr]

from email_worker import (  # noqa: E402
    _IMAP_ENV_VAR,
    _SMTP_ENV_VARS,
    _check_imap_config,
    _check_smtp_config,
    agent,
    manifest,
)


def test_manifest_contains_required_fields() -> None:
    # GIVEN email-worker is imported
    # WHEN manifest() is called
    m = manifest()
    # THEN it contains all required AIP fields
    assert m["name"] == "email-worker"
    assert m["version"] == "0.1.0"
    assert isinstance(m["description"], str) and m["description"]
    assert m["supports_a2a"] is True


def test_manifest_declares_send_email_hitl() -> None:
    # GIVEN email-worker manifest
    # WHEN tools_requiring_approval is inspected
    m = manifest()
    # THEN send-email is listed as requiring HITL approval
    assert "send-email" in m["tools_requiring_approval"]


def test_manifest_declares_expected_skills() -> None:
    # GIVEN email-worker manifest
    # WHEN skills are inspected
    m = manifest()
    skill_ids = [s["id"] for s in m["skills"]]
    # THEN both send-email and read-inbox are declared
    assert "send-email" in skill_ids
    assert "read-inbox" in skill_ids


def test_manifest_no_packages_required() -> None:
    # GIVEN email-worker manifest
    # WHEN packages is inspected
    m = manifest()
    # THEN no third-party packages are required (uses stdlib only)
    assert m.get("packages", []) == []


def test_check_smtp_config_returns_missing_vars(monkeypatch: pytest.MonkeyPatch) -> None:
    # GIVEN none of the SMTP environment variables are set
    for var in _SMTP_ENV_VARS:
        monkeypatch.delenv(var, raising=False)
    # WHEN _check_smtp_config is called
    missing = _check_smtp_config()
    # THEN all SMTP variables are reported as missing
    assert set(missing) == set(_SMTP_ENV_VARS)


def test_check_smtp_config_returns_empty_when_all_set(monkeypatch: pytest.MonkeyPatch) -> None:
    # GIVEN all SMTP environment variables are set
    for var in _SMTP_ENV_VARS:
        monkeypatch.setenv(var, "test-value")
    # WHEN _check_smtp_config is called
    missing = _check_smtp_config()
    # THEN no variables are reported as missing
    assert missing == []


def test_check_imap_config_includes_imap_host(monkeypatch: pytest.MonkeyPatch) -> None:
    # GIVEN SMTP vars are set but IMAP_HOST is not
    for var in _SMTP_ENV_VARS:
        monkeypatch.setenv(var, "test-value")
    monkeypatch.delenv(_IMAP_ENV_VAR, raising=False)
    # WHEN _check_imap_config is called
    missing = _check_imap_config()
    # THEN APOLLIA_IMAP_HOST is reported as missing
    assert _IMAP_ENV_VAR in missing


@pytest.mark.asyncio
async def test_run_fails_with_explicit_message_when_smtp_config_missing(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    # GIVEN none of the SMTP environment variables are configured
    for var in _SMTP_ENV_VARS:
        monkeypatch.delenv(var, raising=False)
    # WHEN run() is called with a send-email task
    task: dict[str, Any] = {"skill": "send-email", "input": {"text": "Send hello"}}
    result = await agent.run(task, ctx=None)
    # THEN run() returns a failure result with MISSING_CONFIG code
    assert result.get("status") == "failed"
    assert "MISSING_CONFIG" in str(result)
    assert any(var in str(result) for var in _SMTP_ENV_VARS)


@pytest.mark.asyncio
async def test_run_fails_with_explicit_message_when_imap_config_missing(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    # GIVEN SMTP vars are set but APOLLIA_IMAP_HOST is not configured
    for var in _SMTP_ENV_VARS:
        monkeypatch.setenv(var, "test-value")
    monkeypatch.delenv(_IMAP_ENV_VAR, raising=False)
    # WHEN run() is called with a read-inbox task
    task: dict[str, Any] = {"skill": "read-inbox", "input": {"text": "Show inbox"}}
    result = await agent.run(task, ctx=None)
    # THEN run() returns a failure result mentioning the missing IMAP variable
    assert result.get("status") == "failed"
    assert _IMAP_ENV_VAR in str(result)


@pytest.mark.asyncio
async def test_run_delegates_to_react_when_config_present(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    # GIVEN all SMTP environment variables are configured
    for var in _SMTP_ENV_VARS:
        monkeypatch.setenv(var, "test-value")
    # WHEN run() is called (react() is mocked to avoid actual LLM calls)
    task: dict[str, Any] = {"skill": "send-email", "input": {"text": "Send hello"}}
    with patch.object(type(agent), "react", new=AsyncMock(return_value="ok")) as mock_react:
        result = await agent.run(task, ctx=object())
        # THEN react() is called (config check passed)
        mock_react.assert_awaited_once()
    assert result.get("status") == "completed"

"""Tests for document-assistant (ReactAgent-based v2)."""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

import pytest

# ---------------------------------------------------------------------------
# Import path setup
# ---------------------------------------------------------------------------

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
if str(_REPO_ROOT / "sdk") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "sdk"))
if str(_REPO_ROOT / "agents") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "agents"))

from apollia.agents import BaseReActAgent  # noqa: E402
from apollia.testing import MockContext  # noqa: E402

_AGENT_PATH = _REPO_ROOT / "agents" / "assistants" / "document-assistant.py"
_spec_module = importlib.util.spec_from_file_location("document_assistant", _AGENT_PATH)
assert _spec_module is not None and _spec_module.loader is not None
document_assistant = importlib.util.module_from_spec(_spec_module)
_spec_module.loader.exec_module(document_assistant)  # type: ignore[union-attr]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _final_answer(text: str) -> str:
    return json.dumps({
        "thought": "Done",
        "action": "final_answer",
        "text": text,
    })


# ---------------------------------------------------------------------------
# Manifest
# ---------------------------------------------------------------------------


def test_manifest_valid() -> None:
    m = document_assistant.manifest()

    assert m["name"] == "document-assistant"
    assert m["version"] == "2.0.0"
    assert m["memory_namespace"] == "document-assistant"
    assert m["agent_type"] == "assistant"
    assert "file_read" in m["tools_required"]
    assert "ask_user" in m["tools_required"]


def test_manifest_exposes_a2a_worker_tools() -> None:
    m = document_assistant.manifest()
    optional = m["tools_optional"]

    # Excel
    assert "a2a:read-excel" in optional
    assert "a2a:analyze-excel" in optional
    # CSV
    assert "a2a:analyze-csv" in optional
    # PDF
    assert "a2a:extract-text" in optional
    # SQL
    assert "a2a:query-sql" in optional


def test_manifest_skills_declared() -> None:
    m = document_assistant.manifest()
    skill_ids = {s["id"] for s in m["skills"]}
    assert "analyze-document" in skill_ids


# ---------------------------------------------------------------------------
# Class-level ReactAgent contract
# ---------------------------------------------------------------------------


def test_is_react_agent_subclass() -> None:
    assert issubclass(document_assistant.DocumentAssistant, BaseReActAgent)


def test_no_hardcoded_routing_table() -> None:
    """The extension routing regex/map should have been removed."""
    for name in (
        "ROUTING_TABLE",
        "_KEYWORD_ROUTING",
        "_FILE_PATH_RE",
        "_FORMAT_PREF_RE",
        "_FORMAT_PREF_MAP",
        "_extract_file_paths",
        "_route_by_extension",
        "_route_by_keywords",
        "_detect_format_pref",
    ):
        assert not hasattr(document_assistant, name), (
            f"Hard-coded routing helper '{name}' should have been removed"
        )


# ---------------------------------------------------------------------------
# Humanize error (pure helper — kept)
# ---------------------------------------------------------------------------


def test_humanize_error_file_not_found() -> None:
    err = FileNotFoundError("no such file or directory: ventes.xlsx")
    msg = document_assistant._humanize_error(err, "ventes.xlsx")

    assert "ventes.xlsx" in msg
    assert "FileNotFoundError" not in msg
    assert "not found" in msg.lower()


def test_humanize_error_corrupt() -> None:
    err = ValueError("corrupt or invalid file format")
    msg = document_assistant._humanize_error(err, "report.xlsx")

    assert "report.xlsx" in msg
    assert "corrupted" in msg.lower() or "format" in msg.lower()
    assert len(msg) < 400


def test_humanize_error_column() -> None:
    err = KeyError("column 'Ventes' not found")
    msg = document_assistant._humanize_error(err, "data.xlsx")

    assert "data.xlsx" in msg
    assert "column" in msg.lower()


def test_humanize_error_generic_fallback() -> None:
    err = RuntimeError("unexpected internal worker failure")
    msg = document_assistant._humanize_error(err, "data.db")

    assert "data.db" in msg
    assert len(msg) < 400


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------


def test_build_system_prompt_mentions_routing_guide() -> None:
    prompt = document_assistant._build_system_prompt({}, [], [])
    assert "a2a:analyze-excel" in prompt
    assert "a2a:extract-text" in prompt
    assert ".xlsx" in prompt


def test_build_system_prompt_with_recent_files() -> None:
    prompt = document_assistant._build_system_prompt(
        format_preferences={"default": "table"},
        recent_files=["/Users/me/ventes.xlsx"],
        available_workers=["excel-worker", "csv-data-worker"],
    )
    assert "ventes.xlsx" in prompt
    assert "excel-worker" in prompt
    assert "table" in prompt.lower()


def test_build_system_prompt_no_workers_warning() -> None:
    prompt = document_assistant._build_system_prompt({}, [], [])
    assert "No workers detected" in prompt or "no workers" in prompt.lower()


# ---------------------------------------------------------------------------
# run() — smoke test
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_returns_completed_on_final_answer() -> None:
    ctx = MockContext.create(
        tools={"file_read": {"content": ""}},
        llm_responses=[{"content": _final_answer(
            "Which file would you like me to analyse?"
        )}],
        memory=True,
    )

    agent = document_assistant.DocumentAssistant()
    result = await agent.run(
        {"input": {"parts": [{"text": "Analyse my sales file."}]}},
        ctx,
    )

    assert result["status"] == "completed"


@pytest.mark.asyncio
async def test_run_fails_without_llm() -> None:
    ctx = MockContext.create()

    agent = document_assistant.DocumentAssistant()
    result = await agent.run(
        {"input": {"parts": [{"text": "Anything"}]}},
        ctx,
    )

    assert result["status"] == "failed"
    assert result["error"]["code"] == "NO_LLM"


# ---------------------------------------------------------------------------
# Module exports
# ---------------------------------------------------------------------------


def test_module_exports() -> None:
    assert callable(document_assistant.manifest)
    assert callable(document_assistant._humanize_error)
    assert callable(document_assistant._build_system_prompt)
    assert isinstance(document_assistant.agent, document_assistant.DocumentAssistant)

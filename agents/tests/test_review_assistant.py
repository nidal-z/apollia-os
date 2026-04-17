"""Tests for review-assistant (ReactAgent-based v2)."""

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

_AGENT_PATH = _REPO_ROOT / "agents" / "assistants" / "review-assistant.py"
_spec_module = importlib.util.spec_from_file_location("review_assistant", _AGENT_PATH)
assert _spec_module is not None and _spec_module.loader is not None
review_assistant = importlib.util.module_from_spec(_spec_module)
_spec_module.loader.exec_module(review_assistant)  # type: ignore[union-attr]


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
    m = review_assistant.manifest()

    assert m["name"] == "review-assistant"
    assert m["version"] == "2.0.0"
    assert m["memory_namespace"] == "review-assistant"
    assert m["agent_type"] == "assistant"
    assert "file_read" in m["tools_required"]
    assert "bash_executor" in m["tools_required"]


def test_manifest_bash_requires_approval() -> None:
    m = review_assistant.manifest()
    assert "bash_executor" in m["tools_requiring_approval"]


def test_manifest_skills_declared() -> None:
    m = review_assistant.manifest()
    skill_ids = {s["id"] for s in m["skills"]}
    assert "review-implementation" in skill_ids


# ---------------------------------------------------------------------------
# Class-level ReactAgent contract
# ---------------------------------------------------------------------------


def test_is_react_agent_subclass() -> None:
    assert issubclass(review_assistant.ReviewAssistant, BaseReActAgent)


def test_no_hardcoded_language_patterns() -> None:
    """The refactor removed all stack-specific regex. None should remain."""
    # Core regression: make sure the module no longer exports the old
    # hard-coded pattern lists.
    for name in (
        "_BLOQUANT_PATTERNS",
        "_ATTENTION_PATTERNS",
        "_FORBIDDEN_DEP_PATTERNS",
        "_CHECKED_LAYER_RE",
        "_DIFF_ADDITION_RE",
    ):
        assert not hasattr(review_assistant, name), (
            f"Stack-specific pattern '{name}' should have been removed"
        )


def test_no_hardcoded_check_functions() -> None:
    """The deterministic check functions should be gone."""
    for name in (
        "check_completeness",
        "check_conformity",
        "check_tests",
        "_detect_test_runner",
        "_parse_test_output",
        "_run_bash",
        "_get_diff",
        "_build_report",
    ):
        assert not hasattr(review_assistant, name), (
            f"Deterministic helper '{name}' should have been removed"
        )


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------


def test_build_system_prompt_with_rules() -> None:
    prompt = review_assistant._build_system_prompt(
        raw_rules="anyhow INTERDIT. Tests obligatoires.",
        tech_stack=["Cargo.toml", "pyproject.toml"],
    )

    assert "anyhow" in prompt
    assert "Cargo.toml" in prompt
    assert "🟢" in prompt and "🟡" in prompt and "🔴" in prompt


def test_build_system_prompt_no_rules_fallback() -> None:
    prompt = review_assistant._build_system_prompt("", [])
    assert "No rules file found" in prompt
    assert "generic checks" in prompt.lower()


def test_build_system_prompt_mentions_discovery_workflow() -> None:
    prompt = review_assistant._build_system_prompt("", [])
    # Key instruction: review-assistant must READ before JUDGING
    assert "discover" in prompt.lower() or "identify the tech stack" in prompt.lower()


# ---------------------------------------------------------------------------
# Bootstrap
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_bootstrap_uses_project_bootstrap_base() -> None:
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": "abc123", "stderr": "", "exit_code": 0},
        },
        memory=True,
    )

    bootstrap = review_assistant.ReviewContextBootstrap()
    snapshot = await bootstrap.run_bootstrap(ctx)

    # Base snapshot keys should always be populated.
    assert "commit_hash" in snapshot
    assert "tech_stack" in snapshot
    assert "has_git" in snapshot


# ---------------------------------------------------------------------------
# run() — smoke test
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_returns_completed_on_final_answer() -> None:
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": "", "stderr": "", "exit_code": 0},
        },
        llm_responses=[{"content": _final_answer(
            "## Review\n🟢 All tests pass. Ready to merge."
        )}],
        memory=True,
    )

    agent = review_assistant.ReviewAssistant()
    result = await agent.run(
        {"input": {"parts": [{"text": "Review the latest diff"}]}},
        ctx,
    )

    assert result["status"] == "completed"


@pytest.mark.asyncio
async def test_run_fails_without_llm() -> None:
    ctx = MockContext.create()

    agent = review_assistant.ReviewAssistant()
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
    assert callable(review_assistant.manifest)
    assert callable(review_assistant._build_system_prompt)
    assert isinstance(review_assistant.agent, review_assistant.ReviewAssistant)

"""Tests for dev-assistant (ReactAgent-based v2)."""

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

_AGENT_PATH = _REPO_ROOT / "agents" / "assistants" / "dev-assistant.py"
_spec_module = importlib.util.spec_from_file_location("dev_assistant", _AGENT_PATH)
assert _spec_module is not None and _spec_module.loader is not None
dev_assistant = importlib.util.module_from_spec(_spec_module)
_spec_module.loader.exec_module(dev_assistant)  # type: ignore[union-attr]


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
    m = dev_assistant.manifest()

    assert m["name"] == "dev-assistant"
    assert m["version"] == "2.0.0"
    assert m["memory_namespace"] == "dev-assistant"
    assert m["agent_type"] == "assistant"
    assert "file_read" in m["tools_required"]
    assert "file_write" in m["tools_required"]
    assert m["supports_a2a"] is True


def test_manifest_exposes_a2a_worker_tools() -> None:
    m = dev_assistant.manifest()
    optional = m["tools_optional"]

    # The LLM must see a2a:* tools so it can delegate to code-worker/git-worker.
    assert "a2a:generate-code" in optional
    assert "a2a:refactor-code" in optional
    assert "a2a:review-code" in optional
    assert "a2a:git-commit" in optional


def test_manifest_skills_cover_both_modes() -> None:
    m = dev_assistant.manifest()
    skill_ids = {s["id"] for s in m["skills"]}
    assert {"explore-codebase", "implement-spec"}.issubset(skill_ids)


def test_manifest_bash_requires_approval() -> None:
    m = dev_assistant.manifest()
    assert "bash_executor" in m["tools_requiring_approval"]


# ---------------------------------------------------------------------------
# Class-level ReactAgent contract
# ---------------------------------------------------------------------------


def test_is_react_agent_subclass() -> None:
    assert issubclass(dev_assistant.DevAssistant, BaseReActAgent)


def test_class_constants() -> None:
    assert dev_assistant.DevAssistant.MAX_STEPS >= 10


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------


def test_build_system_prompt_with_rules() -> None:
    prompt = dev_assistant._build_system_prompt(
        raw_rules="anyhow INTERDIT dans le workspace",
        architecture=["Cargo.toml", "crates/apollia-core/src/lib.rs"],
        recent_files=["crates/apollia-core/src/types.rs"],
    )

    assert "anyhow" in prompt
    assert "Cargo.toml" in prompt
    assert "types.rs" in prompt
    assert "consultant" not in prompt.lower() or "senior developer" in prompt.lower()


def test_build_system_prompt_mentions_a2a_delegation() -> None:
    prompt = dev_assistant._build_system_prompt("", [], [])
    assert "a2a:generate-code" in prompt
    assert "a2a:git-commit" in prompt


def test_build_system_prompt_no_rules_fallback() -> None:
    prompt = dev_assistant._build_system_prompt("", [], [])
    assert "No rules file found" in prompt


# ---------------------------------------------------------------------------
# Bootstrap
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_bootstrap_collects_architecture_and_recent_files() -> None:
    # Bootstrap relies on bash_executor returning different outputs depending
    # on the command. MockToolProxy returns one response per tool; use a
    # separate reply map via monkeypatching ctx.tools.call.
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": "abc\n", "stderr": "", "exit_code": 0},
        },
        memory=True,
    )

    responses = {
        "find": "./Cargo.toml\n./crates/apollia-core/src/lib.rs\n",
        "git": "crates/apollia-core/src/types.rs\n",
    }

    original_call = ctx.tools.call  # type: ignore[union-attr]

    async def _dispatch(name: str, args: dict[str, Any] | None = None) -> Any:
        if name == "bash_executor":
            cmd = (args or {}).get("command", "")
            # "git diff" check must come first — the recent-files command
            # also contains "find" in the fallback branch.
            if "git diff" in cmd or "-newer" in cmd:
                return {"stdout": responses["git"], "stderr": "", "exit_code": 0}
            if "find" in cmd:
                return {"stdout": responses["find"], "stderr": "", "exit_code": 0}
            return {"stdout": "abc\n", "stderr": "", "exit_code": 0}
        return await original_call(name, args)

    ctx.tools.call = _dispatch  # type: ignore[union-attr, assignment]

    bootstrap = dev_assistant.DevContextBootstrap()
    extra = await bootstrap.extra_scopes(ctx, {})

    assert any("Cargo.toml" in m for m in extra["architecture"])
    assert any("types.rs" in f for f in extra["recent_files"])


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
        llm_responses=[{"content": _final_answer("Here is the codebase overview.")}],
        memory=True,
    )

    agent = dev_assistant.DevAssistant()
    result = await agent.run(
        {"input": {"parts": [{"text": "How does the auth module work?"}]}},
        ctx,
    )

    assert result["status"] == "completed"


@pytest.mark.asyncio
async def test_run_fails_without_llm() -> None:
    ctx = MockContext.create()

    agent = dev_assistant.DevAssistant()
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
    assert callable(dev_assistant.manifest)
    assert callable(dev_assistant._build_system_prompt)
    assert isinstance(dev_assistant.agent, dev_assistant.DevAssistant)

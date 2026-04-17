"""Tests for spec-assistant (ReactAgent-based v2)."""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

import pytest

# ---------------------------------------------------------------------------
# Import path setup — must run before any apollia import
# ---------------------------------------------------------------------------

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
if str(_REPO_ROOT / "sdk") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "sdk"))
if str(_REPO_ROOT / "agents") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "agents"))

from apollia.agents import BaseReActAgent  # noqa: E402
from apollia.testing import MockContext  # noqa: E402

# Load spec-assistant via importlib (hyphen in filename prevents direct import).
_AGENT_PATH = _REPO_ROOT / "agents" / "assistants" / "spec-assistant.py"
_spec_module = importlib.util.spec_from_file_location("spec_assistant", _AGENT_PATH)
assert _spec_module is not None and _spec_module.loader is not None
spec_assistant = importlib.util.module_from_spec(_spec_module)
_spec_module.loader.exec_module(spec_assistant)  # type: ignore[union-attr]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _final_answer(text: str) -> str:
    """ReAct JSON for a final_answer action."""
    return json.dumps({
        "thought": "Done",
        "action": "final_answer",
        "text": text,
    })


def _tool_call(tool: str, **args: Any) -> str:
    """ReAct JSON for a tool_call action."""
    return json.dumps({
        "thought": f"Using {tool}",
        "action": "tool_call",
        "tool": tool,
        "args": args,
    })


def _called_tools(ctx: Any) -> list[str]:
    assert ctx.tools is not None
    return [name for name, _ in ctx.tools.calls]


# ---------------------------------------------------------------------------
# Manifest
# ---------------------------------------------------------------------------


def test_manifest_valid() -> None:
    m = spec_assistant.manifest()

    assert m["name"] == "spec-assistant"
    assert m["version"] == "2.0.0"
    assert m["memory_namespace"] == "spec-assistant"
    assert m["execution_mode"] == "auto"
    assert m["agent_type"] == "assistant"
    assert m["supports_a2a"] is True
    assert m["dangerous_tools_allowed"] is False
    assert "file_read" in m["tools_required"]
    assert "file_write" in m["tools_required"]
    assert "ask_user" in m["tools_required"]


def test_manifest_skills_declared() -> None:
    m = spec_assistant.manifest()
    skill_ids = {s["id"] for s in m["skills"]}
    assert {"create-spec", "refine-spec", "list-specs"}.issubset(skill_ids)


def test_manifest_examples_mention_consultant_workflow() -> None:
    m = spec_assistant.manifest()
    assert len(m["examples"]) >= 3
    assert any("spec" in ex.lower() for ex in m["examples"])


# ---------------------------------------------------------------------------
# Class-level ReactAgent contract
# ---------------------------------------------------------------------------


def test_is_react_agent_subclass() -> None:
    assert issubclass(spec_assistant.SpecAssistant, BaseReActAgent)


def test_class_constants() -> None:
    assert spec_assistant.SpecAssistant.MAX_STEPS >= 10
    assert 0.0 <= spec_assistant.SpecAssistant.TEMPERATURE <= 1.0


# ---------------------------------------------------------------------------
# Slugify helper
# ---------------------------------------------------------------------------


def test_slugify_basic() -> None:
    assert spec_assistant.slugify("User Authentication") == "user-authentication"


def test_slugify_accents_removed() -> None:
    assert spec_assistant.slugify("Révision générale") == "revision-generale"


def test_slugify_empty_string_fallback() -> None:
    assert spec_assistant.slugify("") == "spec"


def test_slugify_truncated_to_64_chars() -> None:
    assert len(spec_assistant.slugify("x" * 120)) == 64


# ---------------------------------------------------------------------------
# System prompt composition
# ---------------------------------------------------------------------------


def test_build_system_prompt_with_rules() -> None:
    prompt = spec_assistant._build_system_prompt(
        raw_rules="anyhow INTERDIT dans le workspace\nPas d'unwrap().",
        existing_specs=["user-auth"],
    )

    assert "anyhow" in prompt
    assert "user-auth" in prompt
    assert "consultant" in prompt.lower()
    assert "workspace rules loaded" in prompt.lower()


def test_build_system_prompt_no_rules_fallback() -> None:
    prompt = spec_assistant._build_system_prompt(raw_rules="", existing_specs=None)

    assert "No rules file found" in prompt
    assert "ask_user" in prompt.lower()


def test_build_system_prompt_mentions_file_write_target() -> None:
    prompt = spec_assistant._build_system_prompt(raw_rules="", existing_specs=None)
    assert ".apollia/tasks" in prompt


def test_build_system_prompt_no_code_generation() -> None:
    prompt = spec_assistant._build_system_prompt(raw_rules="", existing_specs=None)
    assert "never generate code" in prompt.lower()


# ---------------------------------------------------------------------------
# Bootstrap
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_spec_bootstrap_lists_existing_specs() -> None:
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {
                "stdout": ".apollia/tasks/user-auth.md\n",
                "stderr": "",
                "exit_code": 0,
            },
        },
        memory=True,
    )

    bootstrap = spec_assistant.SpecContextBootstrap()
    snapshot = await bootstrap.run_bootstrap(ctx)

    assert "user-auth" in snapshot.get("existing_specs", [])
    assert snapshot.get("spec_count", 0) >= 1


@pytest.mark.asyncio
async def test_spec_bootstrap_handles_no_tools() -> None:
    ctx = MockContext.create(memory=True)  # ctx.tools = None

    bootstrap = spec_assistant.SpecContextBootstrap()
    extra = await bootstrap.extra_scopes(ctx, {})
    assert extra == {"existing_specs": [], "spec_count": 0}


# ---------------------------------------------------------------------------
# run() — ReAct smoke test
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_returns_completed_on_final_answer() -> None:
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": "", "stderr": "", "exit_code": 0},
        },
        llm_responses=[{"content": _final_answer("Need more info — please clarify.")}],
        memory=True,
    )

    agent = spec_assistant.SpecAssistant()
    result = await agent.run(
        {"input": {"parts": [{"text": "Write me a spec for a landing page."}]}},
        ctx,
    )

    assert result["status"] == "completed"


@pytest.mark.asyncio
async def test_run_fails_without_llm() -> None:
    # MockContext.create() without llm_responses leaves ctx.llm as None.
    ctx = MockContext.create()

    agent = spec_assistant.SpecAssistant()
    result = await agent.run(
        {"input": {"parts": [{"text": "Anything"}]}},
        ctx,
    )

    assert result["status"] == "failed"
    assert result["error"]["code"] == "NO_LLM"


@pytest.mark.asyncio
async def test_run_fails_without_input() -> None:
    ctx = MockContext.create(
        tools={"file_read": {"content": ""}, "bash_executor": {"stdout": ""}},
        llm_responses=[{"content": _final_answer("unreachable")}],
        memory=True,
    )

    agent = spec_assistant.SpecAssistant()
    result = await agent.run({"input": {"parts": []}}, ctx)

    assert result["status"] == "failed"
    assert result["error"]["code"] == "NO_INPUT"


@pytest.mark.asyncio
async def test_run_preserves_history_in_messages() -> None:
    """Multi-turn: history is passed to react() and the LLM sees it."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": "", "stderr": "", "exit_code": 0},
        },
        llm_responses=[{"content": _final_answer("OK.")}],
        memory=True,
    )

    agent = spec_assistant.SpecAssistant()
    task = {
        "input": {"parts": [{"text": "Now add a blog section."}]},
        "history": [
            {"role": "user", "parts": [{"text": "Spec for a landing page."}]},
            {"role": "agent", "parts": [{"text": "Saved to .apollia/tasks/landing.md"}]},
        ],
    }
    result = await agent.run(task, ctx)

    assert result["status"] == "completed"
    # The LLM must have seen at least the system prompt + history + new user msg
    assert ctx.llm.call_count == 1
    prompts = ctx.llm.prompts[0]
    # System + 2 history + 1 new user = 4 messages
    assert len(prompts) == 4
    assert prompts[0]["role"] == "system"
    assert prompts[1]["role"] == "user"
    assert prompts[2]["role"] == "assistant"
    assert prompts[3]["role"] == "user"
    assert "blog" in prompts[3]["content"]


# ---------------------------------------------------------------------------
# Module-level exports
# ---------------------------------------------------------------------------


def test_module_exports() -> None:
    assert callable(spec_assistant.manifest)
    assert callable(spec_assistant.slugify)
    assert callable(spec_assistant._build_system_prompt)
    assert isinstance(spec_assistant.agent, spec_assistant.SpecAssistant)

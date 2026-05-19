"""Tests for apollia.agents.react — BaseReActAgent and related helpers."""

from __future__ import annotations

from typing import Any

import pytest

from apollia.agents.react import (
    AIPResult,
    BaseReActAgent,
    DEFAULT_MAX_STEPS,
)
from apollia.tools.schemas import NATIVE_TOOL_SCHEMAS
from apollia.utils.hitl import resume_pending_tool
from apollia.utils.parsing import (
    ActionParseError,
    extract_json,
    validate_action,
)


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


class DummyAgent(BaseReActAgent):
    """Minimal concrete subclass for testing."""

    SYSTEM_PROMPT = "You are a test agent."

    def manifest(self) -> dict[str, Any]:
        return {"name": "dummy", "version": "0.1.0"}

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        return AIPResult.completed("done")


# ---------------------------------------------------------------------------
# BaseReActAgent tests
# ---------------------------------------------------------------------------


def test_base_react_agent_is_abstract() -> None:
    """BaseReActAgent cannot be instantiated directly."""
    with pytest.raises(TypeError):
        BaseReActAgent()  # type: ignore[abstract]


def test_dummy_agent_instantiation() -> None:
    """Concrete subclass can be instantiated."""
    agent = DummyAgent()
    assert agent.manifest()["name"] == "dummy"


@pytest.mark.asyncio
async def test_dummy_agent_run() -> None:
    """Concrete subclass run() returns an AIPResult dict."""
    agent = DummyAgent()
    result = await agent.run({}, None)
    assert result["status"] == "completed"


def test_default_max_steps() -> None:
    """MAX_STEPS has a sensible default matching the module constant."""
    agent = DummyAgent()
    assert agent.MAX_STEPS == DEFAULT_MAX_STEPS


def test_default_temperature() -> None:
    """TEMPERATURE has a sensible default."""
    agent = DummyAgent()
    assert agent.TEMPERATURE == 0.3


def test_get_tool_schemas() -> None:
    """get_tool_schemas() returns a list derived from NATIVE_TOOL_SCHEMAS."""
    agent = DummyAgent()
    schemas = agent.get_tool_schemas()
    assert isinstance(schemas, list)
    assert len(schemas) == len(NATIVE_TOOL_SCHEMAS)


def test_system_prompt_override() -> None:
    """Subclass SYSTEM_PROMPT is used."""
    agent = DummyAgent()
    assert agent.SYSTEM_PROMPT == "You are a test agent."


# ---------------------------------------------------------------------------
# AIPResult dict factory tests
# ---------------------------------------------------------------------------


def test_aip_result_completed_returns_dict() -> None:
    """AIPResult.completed() returns a dict with status and output."""
    result = AIPResult.completed("hello")
    assert isinstance(result, dict)
    assert result["status"] == "completed"
    assert result["output"][0]["text"] == "hello"


def test_aip_result_failed_returns_dict() -> None:
    """AIPResult.failed() returns a dict with status and error."""
    result = AIPResult.failed("E001", "broken")
    assert isinstance(result, dict)
    assert result["status"] == "failed"
    assert result["error"]["code"] == "E001"
    assert result["error"]["message"] == "broken"


def test_aip_result_failed_with_details() -> None:
    """AIPResult.failed() accepts optional details."""
    result = AIPResult.failed("E002", "timeout", details={"elapsed": 30})
    assert result["error"]["details"] == {"elapsed": 30}


def test_aip_result_input_required_returns_dict() -> None:
    """AIPResult.input_required() returns a dict for HITL suspension."""
    result = AIPResult.input_required("Approve?", {"tool": "bash"})
    assert isinstance(result, dict)
    assert result["status"] == "input_required"
    assert result["input_required_data"]["prompt"] == "Approve?"
    assert result["input_required_data"]["context"]["tool"] == "bash"


def test_aip_result_input_required_default_context() -> None:
    """AIPResult.input_required() defaults context to empty dict."""
    result = AIPResult.input_required("Continue?")
    assert result["input_required_data"]["context"] == {}


# ---------------------------------------------------------------------------
# Parsing tests
# ---------------------------------------------------------------------------


def test_extract_json_plain() -> None:
    """extract_json() parses a plain JSON string."""
    data = extract_json('{"action": "final_answer", "text": "done"}')
    assert data["action"] == "final_answer"


def test_extract_json_fenced() -> None:
    """extract_json() parses JSON inside a markdown code fence."""
    raw = '```json\n{"action": "tool_call", "tool": "bash"}\n```'
    data = extract_json(raw)
    assert data["tool"] == "bash"


def test_extract_json_embedded() -> None:
    """extract_json() finds JSON embedded in free text."""
    raw = 'Here is my answer: {"action": "final_answer", "text": "hi"} done.'
    data = extract_json(raw)
    assert data["text"] == "hi"


def test_extract_json_failure() -> None:
    """extract_json() returns empty dict on non-JSON input."""
    assert extract_json("no json here") == {}


def test_validate_action_tool_call() -> None:
    """validate_action() accepts a well-formed tool_call."""
    data = validate_action({"action": "tool_call", "tool": "bash"})
    assert data["args"] == {}
    assert data["thought"] == ""


def test_validate_action_final_answer() -> None:
    """validate_action() accepts a well-formed final_answer."""
    data = validate_action({"action": "final_answer", "text": "done"})
    assert data["text"] == "done"


def test_validate_action_shorthand_is_rewritten() -> None:
    """validate_action() treats `action=<tool_name>` (no tool field) as shorthand.

    LLMs commonly collapse the two fields — strict rejection would crash
    the ReAct loop on an otherwise valid tool call. The shorthand is
    rewritten to the canonical form.
    """
    result = validate_action({"action": "ask_user", "args": {"q": 1}})
    assert result["action"] == "tool_call"
    assert result["tool"] == "ask_user"
    assert result["args"] == {"q": 1}


def test_validate_action_missing_action_field() -> None:
    """validate_action() rejects dicts without any `action` field."""
    with pytest.raises(ActionParseError):
        validate_action({"args": {}})


def test_validate_action_missing_tool() -> None:
    """validate_action() rejects canonical tool_call without tool key."""
    with pytest.raises(ActionParseError):
        validate_action({"action": "tool_call"})


def test_validate_action_missing_text() -> None:
    """validate_action() rejects final_answer without text key."""
    with pytest.raises(ActionParseError):
        validate_action({"action": "final_answer"})


# ---------------------------------------------------------------------------
# HITL utility tests
# ---------------------------------------------------------------------------


def test_resume_pending_tool_not_resumed() -> None:
    """resume_pending_tool() returns None for non-resumed tasks."""
    assert resume_pending_tool({"is_resumed": False}) is None


def test_resume_pending_tool_no_response() -> None:
    """resume_pending_tool() returns None when input_response is missing."""
    assert resume_pending_tool({"is_resumed": True}) is None


def test_resume_pending_tool_extracts_tool() -> None:
    """resume_pending_tool() extracts pending tool from a resumed task."""

    class FakeInputResponse:
        """Simulate the runtime's InputResponse object."""

        approved = True
        context = {"tool": "bash_executor", "args": {"command": "ls"}}

    task = {"is_resumed": True, "input_response": FakeInputResponse()}
    result = resume_pending_tool(task)
    assert result is not None
    assert result["tool"] == "bash_executor"
    assert result["args"] == {"command": "ls"}


# ---------------------------------------------------------------------------
# Tool schemas tests
# ---------------------------------------------------------------------------


def test_native_tool_schemas_keys() -> None:
    """NATIVE_TOOL_SCHEMAS contains the expected native tools."""
    assert "bash_executor" in NATIVE_TOOL_SCHEMAS
    assert "file_read" in NATIVE_TOOL_SCHEMAS
    assert "file_write" in NATIVE_TOOL_SCHEMAS
    assert "ask_user" in NATIVE_TOOL_SCHEMAS
    assert "python_executor" in NATIVE_TOOL_SCHEMAS


def test_native_tool_schemas_structure() -> None:
    """Each tool schema has description, parameters, and example."""
    for name, schema in NATIVE_TOOL_SCHEMAS.items():
        assert "description" in schema, f"{name} missing description"
        assert "parameters" in schema, f"{name} missing parameters"
        assert "example" in schema, f"{name} missing example"


# ---------------------------------------------------------------------------
# HITL history persistence tests (FC15)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_save_and_load_history_round_trip() -> None:
    """_save_history + _load_history performs a correct round-trip via remember/recall."""
    from unittest.mock import AsyncMock, MagicMock

    agent = DummyAgent()
    messages = [
        {"role": "user", "content": "hello"},
        {"role": "assistant", "content": "hi"},
        {"role": "user", "content": "bye"},
        {"role": "assistant", "content": "later"},
        {"role": "user", "content": "ok"},
    ]
    store: dict[str, str] = {}

    async def fake_remember(key: str, value: str) -> None:
        store[key] = value

    async def fake_recall(key: str) -> str | None:
        return store.get(key)

    memory = MagicMock()
    memory.remember = AsyncMock(side_effect=fake_remember)
    memory.recall = AsyncMock(side_effect=fake_recall)

    ctx = MagicMock()
    ctx.memory = memory

    task = {"task_id": "task-abc"}

    # WHEN saving 5 messages
    await agent._save_history(task, ctx, messages)

    # THEN loading from a resumed task returns the same 5 messages
    resumed_task = {"task_id": "task-abc", "is_resumed": True}
    loaded = await agent._load_history(resumed_task, ctx)
    assert loaded == messages, f"Expected {messages}, got {loaded}"


@pytest.mark.asyncio
async def test_load_history_unknown_key_returns_empty() -> None:
    """_load_history returns [] when the key does not exist in memory."""
    from unittest.mock import AsyncMock, MagicMock

    agent = DummyAgent()

    memory = MagicMock()
    memory.recall = AsyncMock(return_value=None)

    ctx = MagicMock()
    ctx.memory = memory

    task = {"task_id": "nonexistent-task", "is_resumed": True}
    result = await agent._load_history(task, ctx)
    assert result == []


# ---------------------------------------------------------------------------
# Backward compatibility import test
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# JSON field streamer
# ---------------------------------------------------------------------------


def test_json_streamer_emits_thought_value() -> None:
    """The streamer yields only the chars inside the thought string value."""
    from apollia.agents.react import _JsonFieldStreamer

    streamer = _JsonFieldStreamer({"thought", "text"})
    raw = '{"thought": "Hello world", "action": "final_answer", "text": "done"}'
    emitted = "".join(streamer.feed(raw))
    assert emitted == "Hello worlddone"


def test_json_streamer_handles_chunked_input() -> None:
    """Chunks split in awkward places still produce the same output."""
    from apollia.agents.react import _JsonFieldStreamer

    streamer = _JsonFieldStreamer({"thought"})
    chunks = ['{"thou', 'ght":', ' "Hel', 'lo"', ', "action": "final_answer"}']
    emitted = "".join(d for c in chunks for d in streamer.feed(c))
    assert emitted == "Hello"


def test_json_streamer_respects_escaped_quotes() -> None:
    """Escaped quotes inside the value do not end the emission."""
    from apollia.agents.react import _JsonFieldStreamer

    streamer = _JsonFieldStreamer({"thought"})
    raw = '{"thought": "He said \\"hi\\" then left", "action": "final_answer"}'
    emitted = "".join(streamer.feed(raw))
    assert emitted == 'He said "hi" then left'


def test_json_streamer_skips_non_watched_fields() -> None:
    """Values of non-watched fields (tool, args) are not emitted."""
    from apollia.agents.react import _JsonFieldStreamer

    streamer = _JsonFieldStreamer({"thought"})
    raw = (
        '{"thought": "Calling tool", "action": "tool_call", '
        '"tool": "bash_executor", "args": {"command": "ls"}}'
    )
    emitted = "".join(streamer.feed(raw))
    assert emitted == "Calling tool"


@pytest.mark.skip(reason="apollia_base shim module supprimé post-LOT 13 (rebuild AgentKit)")
def test_backward_compat_import() -> None:
    """Old import path via apollia_base re-exports all public symbols."""
    import sys
    import os

    agents_dir = os.path.join(os.path.dirname(__file__), "..", "..", "agents")
    agents_dir = os.path.normpath(agents_dir)
    sys.path.insert(0, agents_dir)
    try:
        from apollia_base import (  # type: ignore[import-not-found]
            AIPResult as LegacyAIPResult,
            BaseReActAgent as LegacyBase,
            NATIVE_TOOL_SCHEMAS as LegacySchemas,
            resume_pending_tool as legacy_resume,
        )

        assert LegacyAIPResult is AIPResult
        assert LegacyBase is BaseReActAgent
        assert LegacySchemas is NATIVE_TOOL_SCHEMAS
        assert legacy_resume is resume_pending_tool
    finally:
        sys.path.remove(agents_dir)

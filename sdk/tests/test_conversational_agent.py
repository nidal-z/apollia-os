"""Tests for apollia.agents.conversational — ConversationalAgent base class."""

from __future__ import annotations

from typing import Any
from unittest.mock import AsyncMock, MagicMock

import pytest

from apollia.agents.conversational import ConversationalAgent
from apollia.types import AIPResult


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


class GreeterAgent(ConversationalAgent):
    """Minimal concrete subclass for testing."""

    SYSTEM_PROMPT = "You are a friendly greeter."

    def manifest(self) -> dict[str, Any]:
        return {
            "name": "greeter",
            "version": "0.1.0",
            "execution_mode": "direct",
            "tools_required": [],
        }


def _make_ctx(llm_response: str | None = "Hello!") -> MagicMock:
    """Build a mock RuntimeContext with configurable LLM response."""
    ctx = MagicMock()
    if llm_response is not None:
        ctx.llm = AsyncMock()
        ctx.llm.complete = AsyncMock(return_value={"text": llm_response})
    else:
        ctx.llm = None
    ctx.memory = None
    return ctx


# ---------------------------------------------------------------------------
# converse() tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_converse_basic() -> None:
    """converse() returns LLM response and updated history."""
    agent = GreeterAgent()
    ctx = _make_ctx("Hi there!")
    response, history = await agent.converse(ctx, "Hello")
    assert response == "Hi there!"
    assert len(history) == 3  # system + user + assistant
    assert history[-1]["role"] == "assistant"
    assert history[-1]["content"] == "Hi there!"


@pytest.mark.asyncio
async def test_converse_multi_turn() -> None:
    """converse() maintains conversation history across turns."""
    agent = GreeterAgent()
    ctx = _make_ctx("First response")
    _, history = await agent.converse(ctx, "Turn 1")

    ctx.llm.complete = AsyncMock(return_value={"text": "Second response"})
    response, history = await agent.converse(ctx, "Turn 2", history=history)

    assert response == "Second response"
    assert len(history) == 5  # system + user1 + asst1 + user2 + asst2
    user_messages = [m for m in history if m["role"] == "user"]
    assert len(user_messages) == 2


@pytest.mark.asyncio
async def test_converse_no_llm_raises() -> None:
    """converse() raises RuntimeError when ctx.llm is None."""
    agent = GreeterAgent()
    ctx = _make_ctx(llm_response=None)
    with pytest.raises(RuntimeError, match="requires ctx.llm"):
        await agent.converse(ctx, "Hello")


# ---------------------------------------------------------------------------
# run() tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_returns_aip_result() -> None:
    """run() returns AIPResult.completed with LLM response."""
    agent = GreeterAgent()
    ctx = _make_ctx("Bonjour!")
    task = MagicMock()
    task.input = MagicMock()
    task.input.text = "Salut"
    result = await agent.run(task, ctx)
    assert result.status == "completed"
    assert result.text == "Bonjour!"


@pytest.mark.asyncio
async def test_run_no_input_returns_failed() -> None:
    """run() returns AIPResult.failed when task has no input."""
    agent = GreeterAgent()
    ctx = _make_ctx("ignored")
    task = MagicMock(spec=[])  # no attributes at all
    result = await agent.run(task, ctx)
    assert result.status == "failed"
    assert result.error_code == "NO_INPUT"


@pytest.mark.asyncio
async def test_run_no_llm_raises() -> None:
    """run() raises RuntimeError when ctx.llm is None."""
    agent = GreeterAgent()
    ctx = _make_ctx(llm_response=None)
    task = MagicMock()
    task.input = MagicMock()
    task.input.text = "Hello"
    with pytest.raises(RuntimeError, match="requires ctx.llm"):
        await agent.run(task, ctx)


# ---------------------------------------------------------------------------
# on_response() hook tests
# ---------------------------------------------------------------------------


class UpperAgent(ConversationalAgent):
    """Agent that uppercases all responses."""

    SYSTEM_PROMPT = "You respond in uppercase."

    def manifest(self) -> dict[str, Any]:
        return {"name": "upper", "version": "0.1.0"}

    def on_response(self, response: str) -> str:
        return response.upper()


@pytest.mark.asyncio
async def test_on_response_hook() -> None:
    """on_response() post-processes the LLM output."""
    agent = UpperAgent()
    ctx = _make_ctx("hello world")
    response, history = await agent.converse(ctx, "greet me")
    assert response == "HELLO WORLD"
    assert history[-1]["content"] == "HELLO WORLD"


# ---------------------------------------------------------------------------
# Memory persistence tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_converse_records_to_memory() -> None:
    """converse() persists exchange to ctx.memory when available."""
    agent = GreeterAgent()
    ctx = _make_ctx("Hi!")
    ctx.memory = AsyncMock()
    ctx.memory.record = AsyncMock()

    await agent.converse(ctx, "Hello")

    ctx.memory.record.assert_awaited_once()
    call_kwargs = ctx.memory.record.call_args
    assert "user: Hello" in call_kwargs.kwargs["content"]
    assert "assistant: Hi!" in call_kwargs.kwargs["content"]


# ---------------------------------------------------------------------------
# Class-level attribute tests
# ---------------------------------------------------------------------------


def test_abstract_cannot_instantiate() -> None:
    """ConversationalAgent cannot be instantiated directly."""
    with pytest.raises(TypeError):
        ConversationalAgent()  # type: ignore[abstract]


def test_default_class_attributes() -> None:
    """Default class attributes have expected values."""
    agent = GreeterAgent()
    assert agent.MAX_TURNS == 20
    assert agent.TEMPERATURE == 0.7
    assert agent.SYSTEM_PROMPT == "You are a friendly greeter."


def test_import_from_agents_package() -> None:
    """ConversationalAgent is importable from apollia.agents."""
    from apollia.agents import ConversationalAgent as Imported

    assert Imported is ConversationalAgent

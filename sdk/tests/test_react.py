"""Tests for the ``apollia.react`` utility."""

from __future__ import annotations

from typing import Any
from unittest.mock import MagicMock

import pytest

from apollia import react
from apollia.errors import DomainError
from apollia.testing.mocks import MockLlmProxy


# ──────────────────────────────────────────────────────────────────────
# Public API surface
# ──────────────────────────────────────────────────────────────────────


def test_react_importable_from_apollia_root() -> None:
    """``react`` must be exported from the top-level ``apollia`` package."""
    from apollia import react as imported  # noqa: F401

    assert callable(imported)


def test_react_importable_from_apollia_react_module() -> None:
    """``apollia.react.react`` must be the canonical import path."""
    from apollia.react import react as imported  # noqa: F401

    assert callable(imported)


def test_react_listed_in_dunder_all() -> None:
    import apollia

    assert "react" in apollia.__all__


# ──────────────────────────────────────────────────────────────────────
# Test helpers
# ──────────────────────────────────────────────────────────────────────


class _StubCtx:
    """Minimal ctx exposing ``llm`` (a MockLlmProxy) and an optional
    ``events`` interface — enough for the ``react`` happy paths."""

    def __init__(
        self,
        llm: MockLlmProxy,
        *,
        events: MagicMock | None = None,
    ) -> None:
        self.llm = llm
        if events is not None:
            self.events = events


# ──────────────────────────────────────────────────────────────────────
# Happy path
# ──────────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_react_happy_path_returns_final_answer() -> None:
    """GIVEN an LLM that produces a final string,
    WHEN ``react()`` is awaited with a tool descriptor,
    THEN the returned value matches the LLM output verbatim."""
    llm = MockLlmProxy()
    llm.run_tools_responses = ["Paris is the capital of France."]
    ctx = _StubCtx(llm)

    tools: list[dict[str, Any]] = [
        {
            "name": "a2a__web__search",
            "description": "Search the web",
            "input_schema": {"type": "object", "properties": {}},
        }
    ]

    answer = await react(
        ctx,  # type: ignore[arg-type]
        system="You are a helpful assistant.",
        user="What is the capital of France?",
        tools=tools,
        max_steps=5,
        stream=False,
    )

    assert answer == "Paris is the capital of France."
    # The mock recorded exactly one run_tools call with the right shape.
    assert len(llm.run_tools_calls) == 1
    call = llm.run_tools_calls[0]
    assert call["max_iterations"] == 5
    assert call["tools"] == tools
    # Messages should start with the system prompt then the user message.
    assert call["messages"][0]["role"] == "system"
    assert call["messages"][1]["role"] == "user"
    assert call["messages"][1]["content"] == "What is the capital of France?"


@pytest.mark.asyncio
async def test_react_without_tools_passes_empty_list() -> None:
    """``tools=None`` is normalized to an empty list (pure chat mode)."""
    llm = MockLlmProxy()
    llm.run_tools_responses = ["hi"]
    ctx = _StubCtx(llm)

    answer = await react(
        ctx,  # type: ignore[arg-type]
        system="sys",
        user="usr",
        tools=None,
        max_steps=3,
        stream=False,
    )

    assert answer == "hi"
    assert llm.run_tools_calls[0]["tools"] == []


# ──────────────────────────────────────────────────────────────────────
# Budget enforcement
# ──────────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_react_max_steps_zero_raises_domain_error() -> None:
    """``max_steps=0`` is rejected before reaching the LLM."""
    llm = MockLlmProxy()
    llm.run_tools_responses = ["should-not-be-used"]
    ctx = _StubCtx(llm)

    with pytest.raises(DomainError) as exc_info:
        await react(
            ctx,  # type: ignore[arg-type]
            system="sys",
            user="usr",
            max_steps=0,
            stream=False,
        )

    assert exc_info.value.code == "REACT_MAX_STEPS"
    # The LLM must not have been invoked.
    assert llm.run_tools_calls == []


@pytest.mark.asyncio
async def test_react_negative_max_steps_raises_domain_error() -> None:
    llm = MockLlmProxy()
    ctx = _StubCtx(llm)

    with pytest.raises(DomainError) as exc_info:
        await react(
            ctx,  # type: ignore[arg-type]
            system="sys",
            user="usr",
            max_steps=-1,
            stream=False,
        )

    assert exc_info.value.code == "REACT_MAX_STEPS"


# Observability


@pytest.mark.asyncio
async def test_react_stream_true_emits_thought() -> None:
    """When ``stream=True`` and ``ctx.events`` exposes ``emit_thought``,
    the helper surfaces a single marker event (step=0)."""
    llm = MockLlmProxy()
    llm.run_tools_responses = ["done"]
    events = MagicMock()
    ctx = _StubCtx(llm, events=events)

    answer = await react(
        ctx,  # type: ignore[arg-type]
        system="sys",
        user="usr",
        max_steps=2,
        stream=True,
    )

    assert answer == "done"
    events.emit_thought.assert_called_once()
    _args, kwargs = events.emit_thought.call_args
    assert kwargs.get("step") == 0


@pytest.mark.asyncio
async def test_react_stream_false_does_not_emit_thought() -> None:
    """``stream=False`` suppresses the observability marker."""
    llm = MockLlmProxy()
    llm.run_tools_responses = ["done"]
    events = MagicMock()
    ctx = _StubCtx(llm, events=events)

    await react(
        ctx,  # type: ignore[arg-type]
        system="sys",
        user="usr",
        max_steps=2,
        stream=False,
    )

    events.emit_thought.assert_not_called()


@pytest.mark.asyncio
async def test_react_events_failure_does_not_break_run() -> None:
    """A broken ``emit_thought`` implementation must not abort the run."""
    llm = MockLlmProxy()
    llm.run_tools_responses = ["ok"]
    events = MagicMock()
    events.emit_thought.side_effect = RuntimeError("bus down")
    ctx = _StubCtx(llm, events=events)

    answer = await react(
        ctx,  # type: ignore[arg-type]
        system="sys",
        user="usr",
        max_steps=2,
        stream=True,
    )

    assert answer == "ok"

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
    # GIVEN the package root
    # WHEN react is imported from it
    from apollia import react as imported

    # THEN the imported name is callable, not a module or a placeholder
    assert callable(imported)


def test_react_importable_from_apollia_react_module() -> None:
    """``apollia.react.react`` must be the canonical import path."""
    # GIVEN the canonical module path
    # WHEN react is imported from it
    from apollia.react import react as imported

    # THEN the imported name is callable
    assert callable(imported)


def test_react_listed_in_dunder_all() -> None:
    # GIVEN the package root
    import apollia

    # WHEN its __all__ is read
    # THEN react is declared there, so `from apollia import *` carries it
    assert "react" in apollia.__all__


# ──────────────────────────────────────────────────────────────────────
# Test helpers
# ──────────────────────────────────────────────────────────────────────


class _StubCtx:
    """Minimal ctx exposing ``llm`` (a MockLlmProxy) and an optional
    ``events`` interface - enough for the ``react`` happy paths."""

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
    # GIVEN an LLM that answers with a final string, and one tool descriptor
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

    # WHEN react() is awaited with that tool and a budget of five steps
    answer = await react(
        ctx,  # type: ignore[arg-type]
        system="You are a helpful assistant.",
        user="What is the capital of France?",
        tools=tools,
        max_steps=5,
        stream=False,
    )

    # THEN the answer is verbatim, and one run_tools call carried the right shape
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
    # GIVEN an LLM that answers immediately
    llm = MockLlmProxy()
    llm.run_tools_responses = ["hi"]
    ctx = _StubCtx(llm)

    # WHEN react() is awaited with tools=None
    answer = await react(
        ctx,  # type: ignore[arg-type]
        system="sys",
        user="usr",
        tools=None,
        max_steps=3,
        stream=False,
    )

    # THEN the run happens and the LLM received an empty tool list, not None
    assert answer == "hi"
    assert llm.run_tools_calls[0]["tools"] == []


# ──────────────────────────────────────────────────────────────────────
# Budget enforcement
# ──────────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_react_max_steps_zero_raises_domain_error() -> None:
    """``max_steps=0`` is rejected before reaching the LLM."""
    # GIVEN an LLM with a queued answer that must never be consumed
    llm = MockLlmProxy()
    llm.run_tools_responses = ["should-not-be-used"]
    ctx = _StubCtx(llm)

    # WHEN react() is awaited with a budget of zero steps
    with pytest.raises(DomainError) as exc_info:
        await react(
            ctx,  # type: ignore[arg-type]
            system="sys",
            user="usr",
            max_steps=0,
            stream=False,
        )

    # THEN it fails on the budget, and the LLM was never reached
    assert exc_info.value.code == "REACT_MAX_STEPS"
    # The LLM must not have been invoked.
    assert llm.run_tools_calls == []


@pytest.mark.asyncio
async def test_react_negative_max_steps_raises_domain_error() -> None:
    # GIVEN a stub context
    llm = MockLlmProxy()
    ctx = _StubCtx(llm)

    # WHEN react() is awaited with a negative budget
    with pytest.raises(DomainError) as exc_info:
        await react(
            ctx,  # type: ignore[arg-type]
            system="sys",
            user="usr",
            max_steps=-1,
            stream=False,
        )

    # THEN it fails on the budget, like zero does
    assert exc_info.value.code == "REACT_MAX_STEPS"


# Observability


@pytest.mark.asyncio
async def test_react_stream_true_emits_thought() -> None:
    """When ``stream=True`` and ``ctx.events`` exposes ``emit_thought``,
    the helper surfaces a single marker event (step=0)."""
    # GIVEN a context whose events surface records emit_thought calls
    llm = MockLlmProxy()
    llm.run_tools_responses = ["done"]
    events = MagicMock()
    ctx = _StubCtx(llm, events=events)

    # WHEN react() is awaited with stream=True
    answer = await react(
        ctx,  # type: ignore[arg-type]
        system="sys",
        user="usr",
        max_steps=2,
        stream=True,
    )

    # THEN exactly one marker event was emitted, at step 0
    assert answer == "done"
    events.emit_thought.assert_called_once()
    _args, kwargs = events.emit_thought.call_args
    assert kwargs.get("step") == 0


@pytest.mark.asyncio
async def test_react_stream_false_does_not_emit_thought() -> None:
    """``stream=False`` suppresses the observability marker."""
    # GIVEN a context whose events surface records emit_thought calls
    llm = MockLlmProxy()
    llm.run_tools_responses = ["done"]
    events = MagicMock()
    ctx = _StubCtx(llm, events=events)

    # WHEN react() is awaited with stream=False
    await react(
        ctx,  # type: ignore[arg-type]
        system="sys",
        user="usr",
        max_steps=2,
        stream=False,
    )

    # THEN no marker event was emitted
    events.emit_thought.assert_not_called()


@pytest.mark.asyncio
async def test_react_events_failure_does_not_break_run() -> None:
    """A broken ``emit_thought`` implementation must not abort the run."""
    # GIVEN a context whose emit_thought always raises
    llm = MockLlmProxy()
    llm.run_tools_responses = ["ok"]
    events = MagicMock()
    events.emit_thought.side_effect = RuntimeError("bus down")
    ctx = _StubCtx(llm, events=events)

    # WHEN react() is awaited with stream=True
    answer = await react(
        ctx,  # type: ignore[arg-type]
        system="sys",
        user="usr",
        max_steps=2,
        stream=True,
    )

    # THEN the run still returns its answer, so observability cannot break it
    assert answer == "ok"

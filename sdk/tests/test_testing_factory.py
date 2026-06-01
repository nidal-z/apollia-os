"""Tests for apollia.testing.mock factory + new assertion helpers."""

from __future__ import annotations

import sys
import types
from typing import Any

import pytest

from apollia.agent import agent
from apollia.errors import DomainError
from apollia.messages import on_message
from apollia.skills import skill
from apollia.testing import (
    MockContext,
    assert_emitted_thought,
    assert_emitted_token,
    assert_memory_recorded,
    assert_result_completed,
    assert_result_failed,
    assert_result_input_required,
    assert_skill_called,
    assert_template_rendered,
    assert_tool_called,
    mock,
)


# ──────────────────────────────────────────────────────────────────────
# Module-fixture machinery
#
# ``@agent`` refuses to register two different classes inside
# the same Python module - the test_*.py file itself would have an
# ``agent`` symbol clash with the imported decorator.  We therefore
# build fresh, unique fake modules for every fixture class.
# ──────────────────────────────────────────────────────────────────────


_MODULE_COUNTER = 0


def _make_module() -> str:
    global _MODULE_COUNTER
    _MODULE_COUNTER += 1
    name = f"test_testing_factory_mod_{_MODULE_COUNTER}"
    sys.modules[name] = types.ModuleType(name)
    return name


def _make_simple_agent() -> type:
    """Return a ``@agent``-decorated class exercising several ctx surfaces."""
    mod_name = _make_module()

    class SimpleAgent:
        @skill("foo.echo", description="echo input")
        async def echo(self, x: int, *, ctx: Any) -> dict[str, Any]:
            ctx.events.emit_token(f"x={x}")
            ctx.events.emit_thought("computing", step=1)
            await ctx.memory.record("ran foo.echo")
            await ctx.memory.remember("last_x", str(x))
            await ctx.tools.call("noop", {})
            ctx.templates.render("prompt.j2", x=x)
            await ctx.a2a.invoke("child.worker", {"value": x})
            return {"echoed": x}

        @skill("foo.fail", description="always raises")
        async def fail(self, *, ctx: Any) -> dict[str, Any]:
            raise DomainError("bad_input", "missing thing", details={"k": "v"})

        @on_message
        async def reply(self, message: str, history: list, ctx: Any) -> str:
            ctx.events.emit_token("hi")
            return f"echo:{message}"

    SimpleAgent.__module__ = mod_name
    decorated = agent(
        name="simple", version="0.1.0", description="trivial test agent"
    )(SimpleAgent)
    return decorated


def _prepare_ctx(ctx: MockContext) -> MockContext:
    """Wire the bare-minimum responses needed by ``SimpleAgent.echo``."""
    ctx.tools.responses["noop"] = {}
    ctx.templates.templates["prompt.j2"] = "x={{ x }}"
    return ctx


class NotAnAgent:
    """Plain class - not decorated with @agent."""


# ──────────────────────────────────────────────────────────────────────
# mock() factory
# ──────────────────────────────────────────────────────────────────────


def test_mock_raises_on_undecorated_class() -> None:
    # GIVEN a class without @agent
    # WHEN mock() is called
    # THEN ValueError mentions the missing decorator
    with pytest.raises(ValueError, match="not decorated with @agent"):
        mock(NotAnAgent)


def test_mock_returns_instance_and_ctx() -> None:
    # GIVEN a decorated agent class
    AgentCls = _make_simple_agent()
    # WHEN mock() is called
    # THEN we get back (instance, MockContext) with invoke helpers attached
    agent_instance, ctx = mock(AgentCls)
    assert isinstance(agent_instance, AgentCls)
    assert isinstance(ctx, MockContext)
    assert callable(agent_instance.invoke_skill)
    assert callable(agent_instance.invoke_message)


@pytest.mark.asyncio
async def test_invoke_skill_happy_path() -> None:
    agent_instance, ctx = mock(_make_simple_agent())
    _prepare_ctx(ctx)
    result = await agent_instance.invoke_skill("foo.echo", x=42)
    assert_result_completed(result)
    assert result["output"][0]["data"] == {"echoed": 42}


@pytest.mark.asyncio
async def test_invoke_skill_domain_error_to_failed() -> None:
    agent_instance, _ctx = mock(_make_simple_agent())
    result = await agent_instance.invoke_skill("foo.fail")
    assert_result_failed(result, code="bad_input")


@pytest.mark.asyncio
async def test_ctx_llm_responses_queue_consumed() -> None:
    # GIVEN a pre-loaded MockLlmProxy queue
    ctx = MockContext()
    ctx.llm.responses = [{"content": "first"}, {"content": "second"}]
    # WHEN code calls ctx.llm.complete
    r1 = await ctx.llm.complete("p1")
    r2 = await ctx.llm.complete("p2")
    # THEN both responses come out in order
    assert r1.content == "first"
    assert r2.content == "second"


@pytest.mark.asyncio
async def test_ctx_events_emit_token_tracked() -> None:
    agent_instance, ctx = mock(_make_simple_agent())
    _prepare_ctx(ctx)
    await agent_instance.invoke_skill("foo.echo", x=7)
    assert ctx.events.tokens == ["x=7"]


@pytest.mark.asyncio
async def test_ctx_a2a_invoke_tracked() -> None:
    agent_instance, ctx = mock(_make_simple_agent())
    _prepare_ctx(ctx)
    ctx.a2a.invoke_responses["child.worker"] = {
        "status": "completed",
        "output": [],
    }
    await agent_instance.invoke_skill("foo.echo", x=3)
    assert ctx.a2a.invoke_calls == [("child.worker", {"value": 3})]


@pytest.mark.asyncio
async def test_ctx_datasources_preconfigured() -> None:
    # GIVEN pre-loaded datasource values
    ctx = MockContext()
    ctx.datasources.values = {"competitors": [{"name": "x"}]}
    # WHEN code reads the datasource
    data = ctx.datasources.get("competitors")
    # THEN the configured value is returned
    assert data == [{"name": "x"}]
    # AND missing datasources raise FileNotFoundError
    with pytest.raises(FileNotFoundError):
        ctx.datasources.get("missing")


@pytest.mark.asyncio
async def test_invoke_message_dispatches_to_on_message() -> None:
    agent_instance, ctx = mock(_make_simple_agent())
    result = await agent_instance.invoke_message("ping")
    assert_result_completed(result, contains="echo:ping")
    assert ctx.events.tokens == ["hi"]


# ──────────────────────────────────────────────────────────────────────
# Assertion helpers
# ──────────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_assert_skill_called_passes_and_fails() -> None:
    agent_instance, ctx = mock(_make_simple_agent())
    _prepare_ctx(ctx)
    await agent_instance.invoke_skill("foo.echo", x=1)
    assert_skill_called(ctx, "child.worker")
    assert_skill_called(ctx, "child.worker", times=1)
    with pytest.raises(AssertionError, match="was not invoked"):
        assert_skill_called(ctx, "never.invoked")


@pytest.mark.asyncio
async def test_assert_emitted_token_passes_and_fails() -> None:
    agent_instance, ctx = mock(_make_simple_agent())
    _prepare_ctx(ctx)
    await agent_instance.invoke_skill("foo.echo", x=9)
    assert_emitted_token(ctx)
    assert_emitted_token(ctx, contains="x=9")
    with pytest.raises(AssertionError):
        assert_emitted_token(ctx, contains="not-emitted")


@pytest.mark.asyncio
async def test_assert_emitted_thought_passes_and_fails() -> None:
    agent_instance, ctx = mock(_make_simple_agent())
    _prepare_ctx(ctx)
    await agent_instance.invoke_skill("foo.echo", x=1)
    assert_emitted_thought(ctx, contains="computing")
    fresh_ctx = MockContext()
    with pytest.raises(AssertionError, match="No thoughts were emitted"):
        assert_emitted_thought(fresh_ctx)


@pytest.mark.asyncio
async def test_assert_memory_recorded_passes_and_fails() -> None:
    agent_instance, ctx = mock(_make_simple_agent())
    _prepare_ctx(ctx)
    await agent_instance.invoke_skill("foo.echo", x=1)
    # Episodic check
    assert_memory_recorded(ctx)
    # Semantic key check (via .remember)
    assert_memory_recorded(ctx, key="last_x")
    with pytest.raises(AssertionError, match="not remembered"):
        assert_memory_recorded(ctx, key="never_set")


@pytest.mark.asyncio
async def test_assert_tool_called_passes_and_fails() -> None:
    agent_instance, ctx = mock(_make_simple_agent())
    _prepare_ctx(ctx)
    await agent_instance.invoke_skill("foo.echo", x=1)
    assert_tool_called(ctx, "noop")
    assert_tool_called(ctx, "noop", times=1)
    with pytest.raises(AssertionError):
        assert_tool_called(ctx, "never")


@pytest.mark.asyncio
async def test_assert_template_rendered_passes_and_fails() -> None:
    agent_instance, ctx = mock(_make_simple_agent())
    _prepare_ctx(ctx)
    await agent_instance.invoke_skill("foo.echo", x=1)
    assert_template_rendered(ctx, "prompt.j2")
    with pytest.raises(AssertionError, match="was not rendered"):
        assert_template_rendered(ctx, "missing.j2")


def test_assert_result_completed_and_failed_and_input_required() -> None:
    completed = {
        "status": "completed",
        "output": [{"type": "text", "text": "hello world"}],
    }
    assert_result_completed(completed, contains="hello")

    failed = {"status": "failed", "error": {"code": "BOOM"}}
    assert_result_failed(failed)
    assert_result_failed(failed, code="BOOM")
    with pytest.raises(AssertionError):
        assert_result_failed(failed, code="OTHER")

    needs_input = {"status": "input_required"}
    assert_result_input_required(needs_input)
    with pytest.raises(AssertionError):
        assert_result_input_required(completed)

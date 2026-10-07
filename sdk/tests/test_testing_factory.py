"""Tests for ``MockContext`` and the assertion helpers, on a tool and a workflow."""

from __future__ import annotations

from typing import Any

import pytest
from apollia import tool, workflow
from apollia.errors import DomainError
from apollia.testing import (
    MockContext,
    assert_emitted_thought,
    assert_emitted_token,
    assert_memory_recorded,
    assert_result_completed,
    assert_result_failed,
    assert_result_input_required,
    assert_template_rendered,
    assert_tool_called,
)

from tests._typeddict_fixtures import PlainTD


@tool("echo", "Echo a number through several context services.", action="read")
async def echo(ctx: Any, x: int) -> dict[str, Any]:
    ctx.events.emit_token(f"x={x}")
    ctx.events.emit_thought("computing", step=1)
    await ctx.memory.record("ran echo")
    await ctx.memory.remember("last_x", str(x))
    await ctx.tools.call("noop", {})
    ctx.templates.render("prompt.j2", x=x)
    return {"echoed": x}


@tool("fail", "Always raise a domain error.", action="read")
async def fail(ctx: Any) -> dict[str, Any]:
    raise DomainError("bad_input", "missing thing", details={"k": "v"})


@workflow(
    "relay",
    "Hand the inputs to an agent, then to another workflow.",
    inputs=PlainTD,
    steps=["ask", "file"],
    action="read",
)
async def relay(ctx: Any, inputs: PlainTD) -> dict[str, Any]:
    async with ctx.step("ask"):
        answer = await ctx.agents.run("triage", "Sort this message.")
    async with ctx.step("file"):
        filed = await ctx.workflows.run("file-ledger", {"text": answer})
    return {"answer": answer, "filed": filed}


def _prepared_ctx() -> MockContext:
    """A context carrying the bare-minimum responses ``echo`` needs."""
    ctx = MockContext()
    ctx.tools.responses["noop"] = {}
    ctx.templates.templates["prompt.j2"] = "x={{ x }}"
    return ctx


# ──────────────────────────────────────────────────────────────────────
# A tool and a workflow under MockContext
# ──────────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_a_tool_is_awaited_as_written_with_a_mock_context() -> None:
    # GIVEN a context carrying the responses the tool needs
    ctx = _prepared_ctx()
    # WHEN the decorated function is awaited directly
    result = await echo(ctx, x=42)
    # THEN it returns its plain value: nothing wraps it
    assert result == {"echoed": 42}


@pytest.mark.asyncio
async def test_a_tool_that_raises_reaches_the_test_as_its_own_exception() -> None:
    # GIVEN a tool that always raises a domain error
    ctx = MockContext()
    # WHEN it is awaited
    with pytest.raises(DomainError) as raised:
        await fail(ctx)
    # THEN the test reads the code and the details the tool gave
    assert raised.value.code == "bad_input"
    assert raised.value.details == {"k": "v"}


@pytest.mark.asyncio
async def test_a_workflow_reaches_agents_and_workflows_through_the_mock() -> None:
    # GIVEN canned answers for the agent and the workflow the steps call
    ctx = MockContext()
    ctx.agents.answers = {"triage": "urgent"}
    ctx.workflows.results = {"file-ledger": {"rows": 1}}
    # WHEN the workflow runs
    result = await relay(ctx, {"a": 1, "b": "x"})
    # THEN each call is recorded with its target and what it was handed
    assert result == {"answer": "urgent", "filed": {"rows": 1}}
    assert ctx.agents.run_calls == [("triage", "Sort this message.")]
    assert ctx.workflows.run_calls == [("file-ledger", {"text": "urgent"})]


def test_the_mock_context_offers_no_synchronous_call_between_agents() -> None:
    # GIVEN a fresh mock context
    ctx = MockContext()
    # WHEN the services of the former contract are looked up
    # THEN neither ``ctx.a2a`` nor the tool loop of ``ctx.llm`` is there
    assert not hasattr(ctx, "a2a")
    assert not hasattr(ctx.llm, "run" + "_tools")


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
    # GIVEN a tool that emits one token
    ctx = _prepared_ctx()
    # WHEN it runs
    await echo(ctx, x=7)
    # THEN the mock recorded that token and nothing else
    assert ctx.events.tokens == ["x=7"]


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


# ──────────────────────────────────────────────────────────────────────
# Assertion helpers
# ──────────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_assert_emitted_token_passes_and_fails() -> None:
    # GIVEN a tool run that emitted one token
    ctx = _prepared_ctx()
    await echo(ctx, x=9)
    # WHEN the token is asserted, with and without a substring
    assert_emitted_token(ctx)
    assert_emitted_token(ctx, contains="x=9")
    # THEN the matching assertions pass and the absent substring raises
    with pytest.raises(AssertionError):
        assert_emitted_token(ctx, contains="not-emitted")


@pytest.mark.asyncio
async def test_assert_emitted_thought_passes_and_fails() -> None:
    # GIVEN a tool run that emitted one thought
    ctx = _prepared_ctx()
    await echo(ctx, x=1)
    # WHEN the thought is asserted, then asserted on a fresh context
    assert_emitted_thought(ctx, contains="computing")
    fresh_ctx = MockContext()
    # THEN the first passes and the empty context raises
    with pytest.raises(AssertionError, match="No thoughts were emitted"):
        assert_emitted_thought(fresh_ctx)


@pytest.mark.asyncio
async def test_assert_memory_recorded_passes_and_fails() -> None:
    # GIVEN a tool run that recorded an episode and remembered a key
    ctx = _prepared_ctx()
    await echo(ctx, x=1)
    # WHEN each memory write is asserted, then one that never happened
    assert_memory_recorded(ctx)
    assert_memory_recorded(ctx, key="last_x")
    # THEN the two writes pass and the absent key raises
    with pytest.raises(AssertionError, match="not remembered"):
        assert_memory_recorded(ctx, key="never_set")


@pytest.mark.asyncio
async def test_assert_tool_called_passes_and_fails() -> None:
    # GIVEN a tool run that called one native tool once
    ctx = _prepared_ctx()
    await echo(ctx, x=1)
    # WHEN the call is asserted, with and without a count
    assert_tool_called(ctx, "noop")
    assert_tool_called(ctx, "noop", times=1)
    # THEN both pass and the tool that was never called raises
    with pytest.raises(AssertionError):
        assert_tool_called(ctx, "never")


@pytest.mark.asyncio
async def test_assert_template_rendered_passes_and_fails() -> None:
    # GIVEN a tool run that rendered one template
    ctx = _prepared_ctx()
    await echo(ctx, x=1)
    # WHEN that template is asserted, then one that was never rendered
    assert_template_rendered(ctx, "prompt.j2")
    # THEN the first passes and the absent template raises
    with pytest.raises(AssertionError, match="was not rendered"):
        assert_template_rendered(ctx, "missing.j2")


def test_assert_result_completed_and_failed_and_input_required() -> None:
    # GIVEN one result of each of the three statuses
    completed = {
        "status": "completed",
        "output": [{"type": "text", "text": "hello world"}],
    }
    failed = {"status": "failed", "error": {"code": "BOOM"}}
    needs_input = {"status": "input_required"}

    # WHEN each helper is applied to its own status, then to a foreign one
    # THEN every match passes and every mismatch raises
    assert_result_completed(completed, contains="hello")

    assert_result_failed(failed)
    assert_result_failed(failed, code="BOOM")
    with pytest.raises(AssertionError):
        assert_result_failed(failed, code="OTHER")

    assert_result_input_required(needs_input)
    with pytest.raises(AssertionError):
        assert_result_input_required(completed)

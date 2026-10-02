"""Tests for the ``@workflow`` decorator and ``ctx.step``."""

import inspect
from typing import Annotated, Any, NotRequired, TypedDict

import pytest
from apollia import WorkflowCtx, workflow
from apollia.errors import AgentConfigError
from apollia.testing import MockContext
from apollia.types import Ctx
from apollia.workflow_decorator import WORKFLOW_ATTR, WorkflowSpec


class MonthClose(TypedDict):
    folder: Annotated[str, "folder holding the invoices"]
    notify: NotRequired[bool]


def _close_month() -> Any:
    @workflow(
        "close-month",
        "Extract and file the totals of a month.",
        inputs=MonthClose,
        steps=["extract", "file"],
        action="write_or_delete",
    )
    async def close_month(ctx: WorkflowCtx, inputs: MonthClose) -> dict[str, Any]:
        async with ctx.step("extract"):
            answer = await ctx.agents.run("accountant", f"totals of {inputs['folder']}")
        async with ctx.step("file"):
            filed = await ctx.workflows.run("file-ledger", {"text": answer})
        return {"answer": answer, "filed": filed}

    return close_month


def test_workflow_records_its_declaration_and_input_schema() -> None:
    # GIVEN a workflow declared with a TypedDict and two steps
    close_month = _close_month()

    # WHEN its spec is read
    spec = getattr(close_month, WORKFLOW_ATTR)

    # THEN it carries the steps in order and the schema of the TypedDict
    assert isinstance(spec, WorkflowSpec)
    assert spec.name == "close-month"
    assert spec.steps == ("extract", "file")
    assert spec.action == "write_or_delete"
    assert spec.inputs is MonthClose
    assert spec.input_schema == {
        "type": "object",
        "properties": {
            "folder": {"type": "string", "description": "folder holding the invoices"},
            "notify": {"type": "boolean"},
        },
        "required": ["folder"],
        "additionalProperties": False,
    }
    assert inspect.iscoroutinefunction(close_month)


@pytest.mark.asyncio
async def test_workflow_runs_its_declared_steps_through_ctx() -> None:
    # GIVEN a workflow and a mock context answering for an agent and a workflow
    close_month = _close_month()
    ctx = MockContext()
    ctx.agents.answers = {"accountant": "42.00"}
    ctx.workflows.results = {"file-ledger": {"rows": 1}}

    # WHEN it runs
    result = await close_month(ctx, {"folder": "2026-09"})

    # THEN each step reached ctx.agents.run and ctx.workflows.run as written
    assert result == {"answer": "42.00", "filed": {"rows": 1}}
    assert ctx.agents.run_calls == [("accountant", "totals of 2026-09")]
    assert ctx.workflows.run_calls == [("file-ledger", {"text": "42.00"})]


@pytest.mark.asyncio
async def test_ctx_step_refuses_a_step_the_decorator_did_not_declare() -> None:
    # GIVEN a workflow whose body opens a step missing from its list
    @workflow(
        "drifted",
        "Body and list disagree.",
        inputs=MonthClose,
        steps=["extract"],
        action="read",
    )
    async def drifted(ctx: WorkflowCtx, inputs: MonthClose) -> None:
        async with ctx.step("extract"):
            pass
        async with ctx.step("file"):
            pass

    # WHEN it runs
    # THEN the undeclared step is refused, naming the declared ones
    with pytest.raises(AgentConfigError, match=r"step 'file' is not declared.*\['extract'\]"):
        await drifted(MockContext(), {"folder": "x"})


@pytest.mark.asyncio
async def test_a_workflow_called_from_a_workflow_sees_its_own_steps() -> None:
    # GIVEN an inner workflow called directly from an outer one's step
    @workflow("inner", "Inner.", inputs=MonthClose, steps=["only"], action="read")
    async def inner(ctx: WorkflowCtx, inputs: MonthClose) -> str:
        async with ctx.step("only"):
            return "done"

    @workflow("outer", "Outer.", inputs=MonthClose, steps=["call"], action="read")
    async def outer(ctx: WorkflowCtx, inputs: MonthClose) -> str:
        async with ctx.step("call"):
            return await inner(ctx, inputs)

    # WHEN the outer one runs
    result = await outer(MockContext(), {"folder": "x"})

    # THEN the inner one validated its steps against its own list, not the outer's
    assert result == "done"


def test_workflow_ctx_is_a_ctx_with_step() -> None:
    # GIVEN the protocol a workflow body is typed against
    # WHEN its members are read
    # THEN it extends Ctx with step, and Ctx itself publishes agents and workflows
    assert Ctx in WorkflowCtx.__mro__
    assert "step" in vars(WorkflowCtx)
    assert {"agents", "workflows"} <= set(Ctx.__annotations__)


def test_workflow_refuses_a_function_that_is_not_async() -> None:
    # GIVEN a plain def function
    decorate = workflow("sync", "Sync.", inputs=MonthClose, steps=["a"], action="read")

    # WHEN it is decorated
    # THEN it is refused
    with pytest.raises(AgentConfigError, match="must be 'async def'"):

        @decorate
        def sync(ctx: Any, inputs: MonthClose) -> None:
            return None


def test_workflow_refuses_a_signature_other_than_ctx_and_inputs() -> None:
    # GIVEN an async function taking three parameters
    decorate = workflow("three", "Three.", inputs=MonthClose, steps=["a"], action="read")

    # WHEN it is decorated
    # THEN it is refused
    with pytest.raises(AgentConfigError, match=r"exactly two positional parameters"):

        @decorate
        async def three(ctx: Any, inputs: MonthClose, extra: int) -> None:
            return None


@pytest.mark.parametrize(
    ("kwargs", "message"),
    [
        ({"inputs": dict}, "inputs must be a TypedDict"),
        ({"steps": []}, "steps must be a non-empty list"),
        ({"steps": ["a", "a"]}, "declared twice"),
        ({"steps": ["a", " "]}, r"steps\[1\] must be a non-empty string"),
        ({"action": "delete"}, "not a known family"),
    ],
)
def test_workflow_refuses_a_malformed_declaration(kwargs: dict[str, Any], message: str) -> None:
    # GIVEN a declaration with one malformed argument
    arguments: dict[str, Any] = {"inputs": MonthClose, "steps": ["a"], "action": "read"}
    arguments.update(kwargs)

    # WHEN it is declared
    # THEN it is refused at import time
    with pytest.raises(AgentConfigError, match=message):
        workflow("malformed", "Malformed.", **arguments)

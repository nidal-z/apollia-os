"""``@workflow`` decorator - declare a workflow with its steps written ahead.

A workflow is an ``async def (ctx, inputs)`` function whose inputs are a
``TypedDict`` and whose steps are listed on the decorator. Each step of the
body is written ``async with ctx.step("name"):`` and calls ``ctx.llm``,
``ctx.tools``, ``ctx.agents.run``, ``ctx.workflows.run`` or plain Python.

The list on the decorator is what the workflow card shows without running the
code, so the body may not drift from it: ``ctx.step`` refuses a name the
decorator did not declare.

Run by the runtime, each step is reported as it opens, closes or raises: it is
recorded with its run and published on the event bus. Run under a
``MockContext``, the steps report nothing.
"""

import asyncio
import functools
import inspect
from collections.abc import AsyncIterator, Callable, Coroutine
from contextlib import AbstractAsyncContextManager, asynccontextmanager
from dataclasses import dataclass, field
from typing import Any, Final, Protocol, TypeVar, is_typeddict

from apollia._internal.inference import annotation_to_schema
from apollia.errors import AgentConfigError
from apollia.tool_decorator import check_action, check_async, check_description, check_name
from apollia.types import Ctx

__all__ = ["WORKFLOW_ATTR", "WorkflowCtx", "WorkflowSpec", "workflow"]

#: Attribute the decorator stamps on the returned function.
WORKFLOW_ATTR: Final = "__apollia_workflow__"

R = TypeVar("R")


class WorkflowCtx(Ctx, Protocol):
    """The ``ctx`` a workflow body receives: every ``Ctx`` service plus ``step``."""

    def step(self, name: str) -> AbstractAsyncContextManager[None]:
        """Open the declared step ``name``; an undeclared name is refused."""
        ...


@dataclass(frozen=True)
class WorkflowSpec:
    """What ``@workflow`` records about a function.

    Attributes:
        name: Identifier, ``[a-z0-9_-]+``, unique in the package.
        description: What the workflow does.
        action: The most sensitive action family the workflow performs.
        steps: Step names, in order.
        inputs: The ``TypedDict`` of the inputs.
        input_schema: JSON Schema inferred from ``inputs``.
    """

    name: str
    description: str
    action: str
    steps: tuple[str, ...]
    inputs: type
    input_schema: dict[str, Any] = field(default_factory=dict)


class _StepContext:
    """Wraps the runtime ``ctx`` and adds ``step``; every other name is forwarded."""

    def __init__(self, inner: object, spec: WorkflowSpec) -> None:
        self._apollia_inner = inner
        self._apollia_spec = spec

    # REASON(ANN401): forwards to the runtime ctx, whose services each carry
    # their own type; `object` would make every `ctx.llm` a cast.
    def __getattr__(self, name: str) -> Any:  # noqa: ANN401
        return getattr(self._apollia_inner, name)

    def step(self, name: str) -> AbstractAsyncContextManager[None]:
        if name not in self._apollia_spec.steps:
            raise AgentConfigError(
                f"@workflow {self._apollia_spec.name!r}: step {name!r} is not declared; "
                f"declared steps are {list(self._apollia_spec.steps)}"
            )
        return _open_step(self._apollia_inner, name)


@asynccontextmanager
async def _open_step(inner: object, name: str) -> AsyncIterator[None]:
    """Run the body of the step ``name``, reporting it to the runtime.

    The runtime context of a workflow carries a private hook, awaited when the
    step opens, when it closes, and when it raises. Any other context, a
    ``MockContext`` included, has none, and the step reports nothing. A step
    the runtime refuses to open (the run was stopped, its budget is spent)
    raises before its body runs. A cancellation is not reported: the runtime
    stopped the run and closes the step itself.
    """
    hook = getattr(inner, "_apollia_step_hook", None)
    if hook is None:
        yield
        return
    await hook("started", name)
    try:
        yield
    except asyncio.CancelledError:
        raise
    except BaseException as exc:
        await hook("failed", name, f"{type(exc).__name__}: {exc}")
        raise
    await hook("completed", name)


def _check_steps(name: str, steps: object) -> tuple[str, ...]:
    if not isinstance(steps, (list, tuple)) or not steps:
        raise AgentConfigError(f"@workflow {name!r}: steps must be a non-empty list of names")
    seen: list[str] = []
    for index, step in enumerate(steps):
        if not isinstance(step, str) or not step.strip():
            raise AgentConfigError(f"@workflow {name!r}: steps[{index}] must be a non-empty string")
        if step in seen:
            raise AgentConfigError(f"@workflow {name!r}: step {step!r} is declared twice")
        seen.append(step)
    return tuple(seen)


def _check_signature(name: str, fn: Callable[..., Any]) -> None:
    parameters = list(inspect.signature(fn).parameters.values())
    positional = (inspect.Parameter.POSITIONAL_ONLY, inspect.Parameter.POSITIONAL_OR_KEYWORD)
    if len(parameters) != 2 or any(p.kind not in positional for p in parameters):
        raise AgentConfigError(
            f"@workflow {name!r}: function '{getattr(fn, '__name__', '?')}' must take "
            "exactly two positional parameters, (ctx, inputs)"
        )


def workflow(
    name: str,
    description: str,
    *,
    inputs: type,
    steps: list[str],
    action: str,
) -> Callable[
    [Callable[[Any, Any], Coroutine[Any, Any, R]]],
    Callable[[Any, Any], Coroutine[Any, Any, R]],
]:
    """Declare an ``async def (ctx, inputs)`` function as a workflow.

    Example::

        class InvoiceRun(TypedDict):
            folder: Annotated[str, "folder holding the invoices"]

        @workflow(
            "close-month",
            "Extract every invoice of a folder and file the totals.",
            inputs=InvoiceRun,
            steps=["extract", "file"],
            action="write_or_delete",
        )
        async def close_month(ctx: WorkflowCtx, inputs: InvoiceRun) -> dict[str, int]:
            async with ctx.step("extract"):
                ...
            async with ctx.step("file"):
                ...

    Args:
        name: Identifier, ``[a-z0-9_-]+``, unique in the package.
        description: What the workflow does, shown on its card.
        inputs: A ``TypedDict``; its schema is the workflow's input form.
        steps: Step names, in order, unique, as the card shows them.
        action: The most sensitive action family the workflow performs, one of
            :data:`apollia.tool_decorator.ACTION_FAMILIES`.

    Raises:
        AgentConfigError: on an invalid name, an empty description, an unknown
            family, ``inputs`` that is not a ``TypedDict``, an empty or
            duplicated step list, or a function that is not
            ``async def (ctx, inputs)`` or is already a workflow.
        SchemaError: when a field of ``inputs`` cannot become a schema.
    """
    checked_name = check_name("@workflow", name)
    checked_description = check_description("@workflow", checked_name, description)
    checked_action = check_action("@workflow", checked_name, action)
    if not is_typeddict(inputs):
        raise AgentConfigError(f"@workflow {checked_name!r}: inputs must be a TypedDict class")
    checked_steps = _check_steps(checked_name, steps)
    spec = WorkflowSpec(
        name=checked_name,
        description=checked_description,
        action=checked_action,
        steps=checked_steps,
        inputs=inputs,
        input_schema=annotation_to_schema(inputs),
    )

    def decorator(
        fn: Callable[[Any, Any], Coroutine[Any, Any, R]],
    ) -> Callable[[Any, Any], Coroutine[Any, Any, R]]:
        check_async("@workflow", checked_name, fn)
        if getattr(fn, WORKFLOW_ATTR, None) is not None:
            raise AgentConfigError(
                f"@workflow already applied to function '{getattr(fn, '__name__', '?')}'"
            )
        _check_signature(checked_name, fn)

        @functools.wraps(fn)
        async def run(ctx: object, workflow_inputs: object) -> R:
            if isinstance(ctx, _StepContext):
                ctx = ctx._apollia_inner
            return await fn(_StepContext(ctx, spec), workflow_inputs)

        setattr(run, WORKFLOW_ATTR, spec)
        return run

    return decorator

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

Each input is one of the fields the launch form knows: text (``str``), long
text (:data:`apollia.LongText`), a file (:data:`apollia.FileInput`, received as
the absolute path of the file), a date (``datetime.date``, received as a
``date``), a number (``int``, ``float``), yes or no (``bool``), a choice
(``Literal[...]``) or a list of text or numbers. Any other type is refused when
the module is imported. The schema lists the inputs in the order the
``TypedDict`` declares them under ``x-order``, the order the form follows.
"""

import asyncio
import datetime
import functools
import inspect
import typing
from collections.abc import AsyncIterator, Callable, Coroutine
from contextlib import AbstractAsyncContextManager, asynccontextmanager
from dataclasses import dataclass, field
from typing import Any, Final, Protocol, TypeVar, get_args, get_origin, is_typeddict

from apollia._internal.inference import annotation_to_schema
from apollia.errors import AgentConfigError, PayloadError, SchemaError
from apollia.tool_decorator import check_action, check_async, check_description, check_name
from apollia.types import DATE_FORMAT, FILE_PATH_FORMAT, LONG_TEXT_FORMAT, Ctx

__all__ = ["INPUT_ORDER_KEY", "WORKFLOW_ATTR", "WorkflowCtx", "WorkflowSpec", "workflow"]

#: Attribute the decorator stamps on the returned function.
WORKFLOW_ATTR: Final = "__apollia_workflow__"

#: Key of the input schema that lists the inputs in their declared order.
INPUT_ORDER_KEY: Final = "x-order"

#: The input types a workflow may declare, as the refusal of another one names them.
_SUPPORTED_INPUTS: Final = (
    "text (str), long text (apollia.LongText), a file (apollia.FileInput), "
    "a date (datetime.date), a number (int, float), yes or no (bool), "
    "a choice (Literal[...]) or a list (list[str], list[int], list[float])"
)

_TEXT_FORMATS: Final = frozenset({None, DATE_FORMAT, LONG_TEXT_FORMAT, FILE_PATH_FORMAT})

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


def _is_form_field(schema: dict[str, Any]) -> bool:
    """``True`` when ``schema`` is one of the fields the launch form knows."""
    if "enum" in schema:
        return True
    kind = schema.get("type")
    if kind == "string":
        return schema.get("format") in _TEXT_FORMATS
    if kind in ("integer", "number", "boolean"):
        return True
    if kind == "array":
        items = schema.get("items")
        if not isinstance(items, dict):
            return False
        if "enum" in items:
            return True
        if items.get("type") == "string":
            return "format" not in items
        return items.get("type") in ("integer", "number")
    return False


def _input_schema(name: str, inputs: type) -> tuple[dict[str, Any], tuple[str, ...]]:
    """The schema of ``inputs`` and the names of its date inputs.

    Raises:
        AgentConfigError: when an input is of a type the launch form does not
            know, naming the input and the types it may take.
    """
    try:
        hints = typing.get_type_hints(inputs, include_extras=True)
    except (NameError, TypeError) as exc:
        raise AgentConfigError(f"@workflow {name!r}: the inputs cannot be read: {exc}") from exc
    dates: list[str] = []
    for field_name, annotation in hints.items():
        if getattr(get_origin(annotation), "_name", None) in ("Required", "NotRequired"):
            annotation = get_args(annotation)[0]
        try:
            field_schema = annotation_to_schema(annotation)
        except SchemaError as exc:
            raise AgentConfigError(
                f"@workflow {name!r}: input {field_name!r} has an unsupported type "
                f"({annotation!r}); a workflow input is {_SUPPORTED_INPUTS}"
            ) from exc
        if not _is_form_field(field_schema):
            raise AgentConfigError(
                f"@workflow {name!r}: input {field_name!r} has an unsupported type "
                f"({annotation!r}); a workflow input is {_SUPPORTED_INPUTS}"
            )
        if field_schema.get("format") == DATE_FORMAT:
            dates.append(field_name)
    schema = annotation_to_schema(inputs)
    schema[INPUT_ORDER_KEY] = list(hints)
    return schema, tuple(dates)


def _read_dates(name: str, dates: tuple[str, ...], inputs: object) -> object:
    """``inputs`` with each date input given as ``YYYY-MM-DD`` read as a ``date``."""
    if not dates or not isinstance(inputs, dict):
        return inputs
    read = dict(inputs)
    for field_name in dates:
        value = read.get(field_name)
        if not isinstance(value, str):
            continue
        try:
            read[field_name] = datetime.date.fromisoformat(value)
        except ValueError as exc:
            raise PayloadError(
                f"@workflow {name!r}: input {field_name!r} is not a date (YYYY-MM-DD): {value!r}",
                field=field_name,
            ) from exc
    return read


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
        inputs: A ``TypedDict``; its schema is the workflow's input form, and
            each of its inputs one of the types listed in the module docstring.
        steps: Step names, in order, unique, as the card shows them.
        action: The most sensitive action family the workflow performs, one of
            :data:`apollia.tool_decorator.ACTION_FAMILIES`.

    Raises:
        AgentConfigError: on an invalid name, an empty description, an unknown
            family, ``inputs`` that is not a ``TypedDict`` or that declares an
            input of a type the launch form does not know, an empty or
            duplicated step list, or a function that is not
            ``async def (ctx, inputs)`` or is already a workflow.
    """
    checked_name = check_name("@workflow", name)
    checked_description = check_description("@workflow", checked_name, description)
    checked_action = check_action("@workflow", checked_name, action)
    if not is_typeddict(inputs):
        raise AgentConfigError(f"@workflow {checked_name!r}: inputs must be a TypedDict class")
    checked_steps = _check_steps(checked_name, steps)
    input_schema, dates = _input_schema(checked_name, inputs)
    spec = WorkflowSpec(
        name=checked_name,
        description=checked_description,
        action=checked_action,
        steps=checked_steps,
        inputs=inputs,
        input_schema=input_schema,
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
            return await fn(
                _StepContext(ctx, spec), _read_dates(checked_name, dates, workflow_inputs)
            )

        setattr(run, WORKFLOW_ATTR, spec)
        return run

    return decorator

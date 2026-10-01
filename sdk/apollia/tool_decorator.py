"""``@tool`` decorator - declare a Python tool a package ships.

A tool is an ``async def`` function any agent can be granted. The decorator
validates the declaration at import time, infers the input schema from the
signature exactly as ``@skill`` does (``TypedDict``, ``Annotated``, scalars),
and stamps a :class:`ToolSpec` on the function. ``python -m apollia describe``
reads that stamp to write the ``[[tools]]`` descriptor of ``apollia.toml``, so
nothing has to import the code to know what the tool is.
"""

import inspect
import re
from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any, Final, TypeVar

from apollia._internal.inference import signature_to_input_schema
from apollia.errors import AgentConfigError

__all__ = ["ACTION_FAMILIES", "TOOL_ATTR", "ToolSpec", "tool"]

F = TypeVar("F", bound=Callable[..., Any])

#: Attribute the decorator stamps on the function.
TOOL_ATTR: Final = "__apollia_tool__"

#: The action families a tool or a workflow declares, as ``apollia.toml``
#: spells them. Every family except ``read`` makes the access sensitive.
ACTION_FAMILIES: Final[tuple[str, ...]] = (
    "read",
    "read_personal",
    "write_or_delete",
    "send_external",
    "execute_code",
)

# The identifier alphabet of ``apollia.toml``.
_NAME_RE: Final = re.compile(r"^[a-z0-9_-]+$")


@dataclass(frozen=True)
class ToolSpec:
    """What ``@tool`` records about a function.

    Attributes:
        name: Identifier, ``[a-z0-9_-]+``, unique in the package.
        description: What the tool does, for the model and the agent card.
        action: One of :data:`ACTION_FAMILIES`.
        dangerous: The author's own flag; see :func:`tool`.
        examples: Sample input payloads.
        input_schema: JSON Schema inferred from the signature.
    """

    name: str
    description: str
    action: str
    dangerous: bool
    examples: list[dict[str, Any]] = field(default_factory=list)
    input_schema: dict[str, Any] = field(default_factory=dict)

    @property
    def sensitive(self) -> bool:
        """``True`` for every family except ``read``."""
        return self.action != "read"


def check_name(decorator: str, name: object) -> str:
    """Return ``name`` when it is a valid package identifier.

    Raises:
        AgentConfigError: when ``name`` is not a string of ``[a-z0-9_-]+``.
    """
    if not isinstance(name, str) or not _NAME_RE.match(name):
        raise AgentConfigError(f"{decorator} name {name!r} is invalid: it must match [a-z0-9_-]+")
    return name


def check_description(decorator: str, name: str, description: object) -> str:
    """Return ``description`` when it is a non-empty string.

    Raises:
        AgentConfigError: when ``description`` is empty or not a string.
    """
    if not isinstance(description, str) or not description.strip():
        raise AgentConfigError(f"{decorator} {name!r}: description must be a non-empty string")
    return description


def check_action(decorator: str, name: str, action: object) -> str:
    """Return ``action`` when it is one of :data:`ACTION_FAMILIES`.

    Raises:
        AgentConfigError: when ``action`` is not a known family.
    """
    if action not in ACTION_FAMILIES:
        raise AgentConfigError(
            f"{decorator} {name!r}: action {action!r} is not a known family; "
            f"expected one of {', '.join(ACTION_FAMILIES)}"
        )
    return str(action)


def check_async(decorator: str, name: str, fn: object) -> None:
    """Refuse anything but an ``async def`` function.

    Raises:
        AgentConfigError: when ``fn`` is not a coroutine function.
    """
    if not callable(fn):
        raise AgentConfigError(f"{decorator} {name!r} must decorate a callable")
    if not inspect.iscoroutinefunction(fn):
        raise AgentConfigError(
            f"{decorator} {name!r}: function '{getattr(fn, '__name__', '?')}' must be 'async def'"
        )


def _check_examples(name: str, examples: object) -> list[dict[str, Any]]:
    if examples is None:
        return []
    if not isinstance(examples, list):
        raise AgentConfigError(
            f"@tool {name!r}: examples must be a list of dicts, got {type(examples).__name__}"
        )
    for index, item in enumerate(examples):
        if not isinstance(item, dict):
            raise AgentConfigError(
                f"@tool {name!r}: examples[{index}] must be a dict, got {type(item).__name__}"
            )
    return [dict(item) for item in examples]


def tool(
    name: str,
    description: str,
    *,
    action: str,
    dangerous: bool = False,
    examples: list[dict[str, Any]] | None = None,
) -> Callable[[F], F]:
    """Declare an ``async def`` function as a tool the package ships.

    The input schema is inferred from the signature, the ``ctx`` parameter
    excluded, the same way ``@skill`` infers it::

        @tool(
            "extract_totals",
            "Read the totals of an invoice.",
            action="read",
            examples=[{"path": "invoices/2026-09.pdf"}],
        )
        async def extract_totals(
            ctx: Ctx, path: Annotated[str, "path of the invoice"]
        ) -> dict[str, float]: ...

    Args:
        name: Identifier, ``[a-z0-9_-]+``, unique in the package.
        description: What the tool does, for the model and the agent card.
        action: The action family, one of :data:`ACTION_FAMILIES`. It decides
            whether granting the tool is sensitive: every family except
            ``read`` is.
        dangerous: Marks the tool as sensitive. ``apollia.toml`` derives
            sensitivity from the family alone, so the flag cannot make a
            ``read`` tool sensitive and that combination is refused.
        examples: Sample input payloads, each a dict.

    Raises:
        AgentConfigError: on an invalid name, an empty description, an
            unknown family, ``dangerous=True`` on a ``read`` tool, examples
            that are not a list of dicts, or a function that is not
            ``async def`` or is already a tool.
        SchemaError: when a parameter annotation cannot become a schema.
    """
    checked_name = check_name("@tool", name)
    checked_description = check_description("@tool", checked_name, description)
    checked_action = check_action("@tool", checked_name, action)
    if not isinstance(dangerous, bool):
        raise AgentConfigError(f"@tool {checked_name!r}: dangerous must be a bool")
    if dangerous and checked_action == "read":
        raise AgentConfigError(
            f"@tool {checked_name!r}: dangerous=True on a 'read' tool cannot be "
            "declared, since sensitivity follows the family; pick the family "
            "that says what the tool really does"
        )
    checked_examples = _check_examples(checked_name, examples)

    def decorator(fn: F) -> F:
        check_async("@tool", checked_name, fn)
        if getattr(fn, TOOL_ATTR, None) is not None:
            raise AgentConfigError(
                f"@tool already applied to function '{getattr(fn, '__name__', '?')}'"
            )
        spec = ToolSpec(
            name=checked_name,
            description=checked_description,
            action=checked_action,
            dangerous=dangerous,
            examples=checked_examples,
            input_schema=signature_to_input_schema(fn),
        )
        setattr(fn, TOOL_ATTR, spec)
        return fn

    return decorator

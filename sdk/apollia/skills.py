"""``@skill`` decorator — mark a method as an A2A-invocable skill.

See ADR-098 (decorator-first) and ADR-099 (signature = schema). The
decorator stamps a marker on the method; the manifest builder (LOT 1)
walks those markers to assemble the canonical agent manifest.
"""

from __future__ import annotations

import inspect
import re
from collections.abc import Callable
from typing import Any, TypeVar

from apollia._internal.manifest import SKILL_ATTR
from apollia.errors import AgentConfigError

__all__ = ["skill"]

F = TypeVar("F", bound=Callable[..., Any])

# Dot-namespaced lowercase identifiers (snake_case segments).
_SKILL_ID_RE = re.compile(r"^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$")


def _validate_skill_id(skill_id: Any) -> str:
    if not isinstance(skill_id, str) or not skill_id:
        raise AgentConfigError(
            "@skill requires a non-empty string skill_id"
        )
    if not _SKILL_ID_RE.match(skill_id):
        raise AgentConfigError(
            f"@skill id {skill_id!r} is invalid: must match "
            r"^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$ "
            "(lowercase, underscores, dot-separated namespaces)"
        )
    return skill_id


def skill(
    skill_id: str,
    *,
    description: str = "",
    requires_approval: bool = False,
    dangerous: bool = False,
) -> Callable[[F], F]:
    """Mark a method as an A2A skill exposed by the agent.

    The method signature (excluding ``self`` and ``ctx``) is introspected
    at decoration time to generate the JSON Schema for input validation
    (cf. ADR-099). The method must be ``async def``.

    Args:
        skill_id: Dot-namespaced unique identifier (e.g. ``"pdf.read_text"``).
        description: Human-readable description for A2A discovery.
        requires_approval: HITL gate before invocation.
        dangerous: Marks the skill as potentially destructive (display warning).

    Raises:
        AgentConfigError: if ``skill_id`` is empty, contains invalid chars,
            or the method is not async, or ``@skill`` is applied twice on
            the same method.
    """
    validated_id = _validate_skill_id(skill_id)
    if not isinstance(description, str):
        raise AgentConfigError("@skill description must be a string")

    def decorator(fn: F) -> F:
        if not callable(fn):
            raise AgentConfigError(
                f"@skill must decorate a callable, got {type(fn).__name__}"
            )
        if not inspect.iscoroutinefunction(fn):
            raise AgentConfigError(
                f"@skill {validated_id!r}: method '{getattr(fn, '__name__', '?')}' "
                "must be 'async def'"
            )
        if getattr(fn, SKILL_ATTR, None) is not None:
            raise AgentConfigError(
                f"@skill already applied to method "
                f"'{getattr(fn, '__name__', '?')}'"
            )
        setattr(
            fn,
            SKILL_ATTR,
            {
                "id": validated_id,
                "description": description,
                "requires_approval": bool(requires_approval),
                "dangerous": bool(dangerous),
            },
        )
        return fn

    return decorator

"""``@skill`` decorator - mark a method as an A2A-invocable skill.

The decorator stamps a marker on the method; the manifest builder
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


def _validate_examples(
    skill_id: str, examples: list[dict[str, Any]] | None
) -> list[dict[str, Any]]:
    """Validate the optional ``examples=`` argument and return a normalized list.

    Each example must be a ``dict`` representing a sample payload. The
    SDK does not validate examples against the inferred schema - author
    is responsible (validating here would create a chicken-and-egg cycle
    since the schema isn't built until ``collect_skills`` runs).
    """
    if examples is None:
        return []
    if not isinstance(examples, list):
        raise AgentConfigError(
            f"@skill {skill_id!r}: examples must be a list of dicts, "
            f"got {type(examples).__name__}"
        )
    for idx, item in enumerate(examples):
        if not isinstance(item, dict):
            raise AgentConfigError(
                f"@skill {skill_id!r}: examples[{idx}] must be a dict, "
                f"got {type(item).__name__}"
            )
    # Shallow copy each example to defensively isolate from caller mutations.
    return [dict(ex) for ex in examples]


def skill(
    skill_id: str,
    *,
    description: str = "",
    requires_approval: bool = False,
    dangerous: bool = False,
    examples: list[dict[str, Any]] | None = None,
) -> Callable[[F], F]:
    """Mark a method as an A2A skill exposed by the agent.

    The method signature (excluding ``self`` and ``ctx``) is introspected
    at decoration time to generate the JSON Schema for input validation
    The method must be ``async def``.

    Args:
        skill_id: Dot-namespaced unique identifier (e.g. ``"pdf.read_text"``).
        description: Human-readable description for A2A discovery. When
            omitted, the manifest falls back to the first line of the
            method's docstring.
        requires_approval: HITL gate before invocation.
        dangerous: Marks the skill as potentially destructive (display warning).
        examples: Optional list of sample payload dicts shown alongside
            the input schema in the manifest. Useful for steering small
            LLMs that struggle with JSON Schema alone. Each entry must be
            a dict; the SDK does not validate it against the schema so
            authors are responsible for keeping examples in sync.

    Raises:
        AgentConfigError: if ``skill_id`` is empty, contains invalid chars,
            or the method is not async, or ``@skill`` is applied twice on
            the same method, or ``examples`` is not a list of dicts.
    """
    validated_id = _validate_skill_id(skill_id)
    if not isinstance(description, str):
        raise AgentConfigError("@skill description must be a string")
    validated_examples = _validate_examples(validated_id, examples)

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
                "examples": validated_examples,
            },
        )
        return fn

    return decorator

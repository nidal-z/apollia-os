"""Typed exception hierarchy for the Apollia SDK.

All SDK-raised exceptions inherit from :class:`AgentError`. The dispatch
boundary (see ``apollia._internal.dispatch``) catches these and translates
them into structured ``AIPResult`` dicts that the Rust runtime consumes.

See ADR-100 for the full design rationale.
"""

from __future__ import annotations

from typing import Any

__all__ = [
    "AgentError",
    "DomainError",
    "NeedHumanInput",
    "PayloadError",
    "SchemaError",
    "SkillNotFound",
    "AgentConfigError",
]


class AgentError(Exception):
    """Base class for all Apollia agent errors."""


class DomainError(AgentError):
    """Domain-level failure surfaced to the runtime as ``AIPResult.failed``.

    The SDK dispatch boundary catches this exception and produces an
    ``AIPResult`` with ``status == "failed"`` carrying ``code``, ``message``
    and ``details``. Codes should be stable snake_case or SCREAMING_SNAKE
    identifiers (e.g. ``"FILE_NOT_FOUND"``) so they can be branched on by
    callers.
    """

    def __init__(
        self,
        code: str,
        message: str,
        details: dict[str, Any] | None = None,
    ) -> None:
        super().__init__(message)
        self.code: str = code
        self.message: str = message
        self.details: dict[str, Any] | None = details


class NeedHumanInput(AgentError):
    """Suspend execution awaiting human input (Human-In-The-Loop).

    Trapped by the SDK and formatted into an ``AIPResult`` with
    ``status == "input_required"`` carrying ``prompt`` and ``context``.
    The runtime persists ``context`` verbatim and restitutes it when the
    task resumes.
    """

    def __init__(self, prompt: str, context: dict[str, Any] | None = None) -> None:
        super().__init__(prompt)
        self.prompt: str = prompt
        self.context: dict[str, Any] = context if context is not None else {}


class PayloadError(AgentError):
    """Input payload validation failed.

    Raised by the inference validator when a task payload does not match
    the schema inferred from the handler signature.
    """

    def __init__(
        self,
        message: str,
        field: str | None = None,
        details: dict[str, Any] | None = None,
    ) -> None:
        super().__init__(message)
        self.message: str = message
        self.field: str | None = field
        self.details: dict[str, Any] | None = details


class SchemaError(AgentError):
    """A handler signature could not be inferred to a valid JSON Schema."""


class SkillNotFound(AgentError):
    """Requested skill ID is not registered on this agent."""

    def __init__(self, skill_id: str, known: list[str] | None = None) -> None:
        super().__init__(f"Skill not found: {skill_id}")
        self.skill_id: str = skill_id
        self.known: list[str] = known if known is not None else []


class AgentConfigError(AgentError):
    """Agent decorator configuration is invalid.

    Raised at import / load time so misconfigurations are caught before
    the runtime ever schedules a task on the agent (fail-fast, ADR-110).
    """

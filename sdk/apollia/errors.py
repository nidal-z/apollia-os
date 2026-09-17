"""Typed exception hierarchy for the Apollia SDK.

All SDK-raised exceptions inherit from :class:`AgentError`. The dispatch
boundary (see ``apollia._internal.dispatch``) catches these and translates
them into structured ``AIPResult`` dicts that the Rust runtime consumes.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from apollia.hitl import HitlPayload

__all__ = [
    "AgentConfigError",
    "AgentError",
    "DomainError",
    "NeedHumanInput",
    "PayloadError",
    "SchemaError",
    "SkillNotFound",
    "StructuredOutputError",
    "ToolApprovalDenied",
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
        """Build a domain failure.

        Args:
            code: Stable identifier callers can branch on.
            message: Human-readable explanation.
            details: Structured context carried into ``AIPResult.error``.
        """
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

    def __init__(
        self,
        prompt: str,
        context: dict[str, Any] | None = None,
        *,
        payload: HitlPayload | None = None,
        from_tool_call: bool = False,
    ) -> None:
        """Suspend the run and ask the human a question.

        Args:
            prompt: What the human is being asked.
            context: State to persist verbatim and restitute on resume, through
                ``ctx.input_response["context"]``. The way to carry what an
                earlier pause learned into a later one.
            payload: A typed question or approval, see :mod:`apollia.hitl`.
                Checked by the runtime when the agent pauses: an invalid one
                fails the task with ``INVALID_INPUT_PAYLOAD``.
            from_tool_call: Set by the runtime, never by an agent, on the pause
                ``ctx.tools.call`` raises for a call that needs an approval.
                Left uncaught with no ``context`` of its own, such a pause
                keeps the context the run was resumed with, so an agent
                resumed from its own pause does not lose its state to the
                engine's.
        """
        super().__init__(prompt)
        self.prompt: str = prompt
        self.context: dict[str, Any] = context if context is not None else {}
        self.payload: HitlPayload | None = payload
        self.from_tool_call: bool = from_tool_call


class ToolApprovalDenied(AgentError):
    """An operator declined the approval a tool call paused on.

    Raised by ``ctx.tools.call`` when the task is resumed from an approval of
    that very call and the operator said no. The call was not executed.
    Catch it to continue without the tool; left uncaught, the task fails.
    """

    def __init__(
        self,
        message: str,
        *,
        tool: str = "",
        reason: str | None = None,
    ) -> None:
        """Report a declined tool approval.

        Args:
            message: The full, human-readable refusal.
            tool: The gesture that was declined, ``<server>/<tool>`` for MCP.
            reason: The operator's reason, when one was given.
        """
        super().__init__(message)
        self.message: str = message
        self.tool: str = tool
        self.reason: str | None = reason


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
        """Report a payload that does not match the inferred schema.

        Args:
            message: What is wrong with the payload.
            field: Dotted path to the offending field, when one is known.
            details: Structured context carried into ``AIPResult.error``.
        """
        super().__init__(message)
        self.message: str = message
        self.field: str | None = field
        self.details: dict[str, Any] | None = details


class SchemaError(AgentError):
    """A handler signature could not be inferred to a valid JSON Schema."""


class StructuredOutputError(AgentError):
    """A ``ctx.llm`` call with a ``schema`` could not deliver a valid value.

    Raised by the bridge, never constructed by an agent. Three situations reach
    it, told apart by :attr:`kind`:

    - ``"schema_unsupported"``: the schema cannot be turned into a decoding
      constraint (``anyOf``, ``$ref``, an untyped node). Raised before the
      model is called, so no tokens were spent.
    - ``"backend_unsupported"``: the resolved backend has no structured output
      mode. The constraint is refused rather than dropped in silence.
    - ``"response_invalid"``: the answer came back and the schema refuses it.

    :attr:`path` is the JSON path of the offending node, so a caller branches on
    a field instead of matching on a sentence::

        try:
            plan = await ctx.llm.complete(messages, schema=PLAN_SCHEMA)
        except StructuredOutputError as e:
            if e.kind == "response_invalid":
                ctx.logger.warning("model missed %s: %s", e.path, e.reason)

    Nothing is retried on your behalf: whether a second attempt is worth its
    tokens is a decision for the agent, not for the runtime.
    """

    def __init__(
        self,
        message: str,
        *,
        kind: str = "response_invalid",
        path: str = "$",
        reason: str = "",
    ) -> None:
        """Report a structured-output failure.

        Args:
            message: The full, human-readable failure.
            kind: Which of the three situations occurred.
            path: JSON path of the offending node, ``$`` for the document.
            reason: What was expected there, and what was found.
        """
        super().__init__(message)
        self.message: str = message
        self.kind: str = kind
        self.path: str = path
        self.reason: str = reason


class SkillNotFound(AgentError):
    """Requested skill ID is not registered on this agent."""

    def __init__(self, skill_id: str, known: list[str] | None = None) -> None:
        """Report an unknown skill.

        Args:
            skill_id: The skill that was requested.
            known: The skills the agent does register, for the error message.
        """
        super().__init__(f"Skill not found: {skill_id}")
        self.skill_id: str = skill_id
        self.known: list[str] = known if known is not None else []


class AgentConfigError(AgentError):
    """Agent decorator configuration is invalid.

    Raised at import / load time so misconfigurations are caught before
    the runtime ever schedules a task on the agent (fail-fast).
    """

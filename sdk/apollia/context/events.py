"""ctx.events — typed event emission."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class EventsInterface(Protocol):
    """Public typed events for streaming, ReAct observability, error reporting.

    Only the four canonical event kinds are exposed.
    Internal lifecycle events (``task_started``, ``step_completed``, ...)
    are emitted by the runtime, not by the agent.
    """

    def emit_token(self, delta: str) -> None:
        """Streaming token chunk."""
        ...

    def emit_thought(self, text: str, *, step: int) -> None:
        """Agent reasoning trace (ReAct observability)."""
        ...

    def emit_retry(self, *, step: int, reason: str, count: int) -> None: ...

    def emit_action_parse_error(
        self,
        *,
        step: int,
        raw: str,
        fatal: bool = False,
    ) -> None: ...

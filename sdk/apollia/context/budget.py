"""ctx.budget - read-only step budget view."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class BudgetView(Protocol):
    """Runtime step budget tracking, read-only from the agent's perspective.

    The actual enforcement happens in the Rust runtime (StepBudget actor).
    This view lets agents introspect remaining budget without bypassing
    the non-negotiable guard-rails (Principle 7).
    """

    @property
    def steps_remaining(self) -> int:
        """Reasoning steps left before the runtime halts the run."""
        ...

    @property
    def tool_calls_remaining(self) -> int:
        """Tool invocations left before the runtime halts the run."""
        ...

    @property
    def elapsed_seconds(self) -> float:
        """Wall-clock seconds since the run started."""
        ...

    @property
    def wall_clock_remaining(self) -> float | None:
        """Seconds left on the wall-clock deadline, or None when uncapped."""
        ...

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
    def steps_remaining(self) -> int: ...

    @property
    def tool_calls_remaining(self) -> int: ...

    @property
    def elapsed_seconds(self) -> float: ...

    @property
    def wall_clock_remaining(self) -> float | None: ...

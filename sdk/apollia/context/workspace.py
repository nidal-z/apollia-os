"""ctx.workspace — workspace context (APOLLIA.md, git, sections)."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class WorkspaceContext(Protocol):
    """Snapshot of the workspace at task start.

    Exposes the parsed ``APOLLIA.md`` (project rules) and named sections.
    The snapshot is immutable for the duration of the task.
    """

    @property
    def rules(self) -> str | None:
        """APOLLIA.md content (alias for :attr:`apollia_md`)."""
        ...

    @property
    def apollia_md(self) -> str | None: ...

    def get(self, title: str) -> str | None:
        """Custom section by title."""
        ...

    @property
    def sections(self) -> list[dict[str, str]]: ...

"""ctx.workspace - workspace context (APOLLIA.md, git, sections)."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class WorkspaceContext(Protocol):
    """The sections collected from a workspace, by title.

    The rules file of a workspace (``APOLLIA.md`` by default) is the section
    titled ``Project rules``; other sections carry their own title. The
    runtime collects no workspace for the code of a package, so this view is
    never attached and ``ctx.workspace`` is ``None``.
    """

    @property
    def rules(self) -> str | None:
        """The ``Project rules`` section (the rules file), or ``None`` when absent."""
        ...

    @property
    def apollia_md(self) -> str | None:
        """Alias for :attr:`rules`."""
        ...

    def get(self, title: str) -> str | None:
        """Custom section by title."""
        ...

    @property
    def sections(self) -> list[dict[str, str]]:
        """Every section, each as a ``{"title", "content"}`` mapping."""
        ...

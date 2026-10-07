"""ctx.templates - runtime Jinja2 template rendering."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class TemplatesInterface(Protocol):
    """Runtime Jinja2 template rendering.

    A template is one the context was given, addressed by its name.
    """

    def render(self, name: str, **context: object) -> str:
        """Render a declared template.

        Args:
            name: Name of a template the context was given.
            **context: Variables exposed to the template.

        Returns:
            The rendered text.
        """
        ...

    def has(self, name: str) -> bool:
        """Whether ``name`` is declared and compiled in memory."""
        ...

    def list_names(self) -> list[str]:
        """Return the names of every template the manifest declares."""
        ...

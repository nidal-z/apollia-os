"""ctx.templates - runtime Jinja2 template rendering."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class TemplatesInterface(Protocol):
    """Runtime Jinja2 template rendering.

    A template is one the context was given, addressed by its name. No
    template is given today: ``apollia.toml`` has no section that names one,
    and the runtime builds this service empty, so on every binary this project
    ships ``render`` raises :class:`FileNotFoundError`, ``has`` returns
    ``False`` and ``list_names`` returns an empty list.
    """

    def render(self, name: str, **context: object) -> str:
        """Render a template the context was given.

        Args:
            name: Name of a template the context was given.
            **context: Variables exposed to the template.

        Returns:
            The rendered text.

        Raises:
            FileNotFoundError: ``name`` was not given to the context, or its
                file could not be compiled.
        """
        ...

    def has(self, name: str) -> bool:
        """Whether ``name`` was given to the context and compiled in memory."""
        ...

    def list_names(self) -> list[str]:
        """Return the names of every template the context was given."""
        ...

"""ctx.templates - runtime Jinja2 template rendering."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class TemplatesInterface(Protocol):
    """Runtime Jinja2 template rendering.

    Templates are declared via ``@agent(templates=(...))`` and resolved
    from the agent package's ``templates/`` directory at task startup.
    """

    def render(self, name: str, **context: object) -> str:
        """Render a declared template.

        Args:
            name: Template name as declared in ``@agent(templates=(...))``.
            **context: Variables exposed to the template.

        Returns:
            The rendered text.
        """
        ...

    def list_names(self) -> list[str]:
        """Return the names of every template the manifest declares."""
        ...

"""ctx.templates - runtime Jinja2 template rendering."""

from __future__ import annotations

from typing import Any, Protocol, runtime_checkable


@runtime_checkable
class TemplatesInterface(Protocol):
    """Runtime Jinja2 template rendering.

    Templates are declared via ``@agent(templates=(...))`` and resolved
    from the agent package's ``templates/`` directory at task startup.
    """

    def render(self, name: str, **context: Any) -> str: ...

    def list_names(self) -> list[str]: ...

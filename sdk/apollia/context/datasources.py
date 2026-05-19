"""ctx.datasources — runtime YAML datasources (ADR-103)."""

from __future__ import annotations

from typing import Any, Protocol, runtime_checkable


@runtime_checkable
class DatasourcesInterface(Protocol):
    """Runtime access to YAML datasources declared in ``@agent(datasources=(...))``."""

    def get(self, name: str) -> Any:
        """Load datasource by name.

        Returns parsed YAML (``dict`` / ``list`` / ``str`` / ``int`` / ...).
        Raises :class:`FileNotFoundError` if ``name`` is not declared in the
        agent manifest.
        """
        ...

    def list_names(self) -> list[str]: ...

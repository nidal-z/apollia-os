"""ctx.datasources - runtime YAML datasources."""

from __future__ import annotations

from typing import Any, Protocol, runtime_checkable


@runtime_checkable
class DatasourcesInterface(Protocol):
    """Runtime access to YAML datasources declared in ``@agent(datasources=(...))``."""

    # REASON(ANN401): the return is parsed YAML of a shape only the agent author
    # knows. `object` would force every caller to cast before indexing, which is
    # noise on the most common line of an agent. `Any` is the contract here.
    def get(self, name: str) -> Any:  # noqa: ANN401
        """Load datasource by name.

        Returns parsed YAML (``dict`` / ``list`` / ``str`` / ``int`` / ...).
        Raises :class:`FileNotFoundError` if ``name`` is not declared in the
        agent manifest.
        """
        ...

    def list_names(self) -> list[str]:
        """Return the names of every datasource the manifest declares."""
        ...

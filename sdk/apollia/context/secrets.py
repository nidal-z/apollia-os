"""ctx.secrets - read-only credentials access."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class SecretsInterface(Protocol):
    """Read-only access to the credentials the context was granted.

    The keys the context was granted are the only authority: a key outside
    them returns ``None`` even when a value is stored under it. No key is
    granted today. ``apollia.toml`` has no section that names one, and the
    runtime builds this service with no key and no credential store, so on
    every binary this project ships ``get`` returns ``None``, ``has`` returns
    ``False`` and ``list_names`` returns an empty list. A credential a
    connection or a native tool needs is held by that connection or tool, not
    read through ``ctx``. Agents never write to this surface.
    """

    def get(self, key: str) -> str | None:
        """Returns the secret value, or ``None`` if not configured."""
        ...

    def has(self, key: str) -> bool:
        """Whether ``key`` was granted and currently holds a value.

        Strictly equivalent to ``ctx.secrets.get(key) is not None``.
        """
        ...

    def list_names(self) -> list[str]:
        """Return the secret keys the context was granted, never their values."""
        ...

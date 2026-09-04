"""ctx.secrets - read-only credentials access."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class SecretsInterface(Protocol):
    """Read-only access to credentials declared in ``@agent(secrets=(...))``.

    Every ``get`` reads the encrypted credential store at call time; there is
    no snapshot taken at task startup. The store is the ``governance.db``
    SQLite database and the key file beside it, opened once when the runtime
    starts, and a runtime that could not open it hands back ``None`` for every
    key. The manifest declaration is the only authority: a key it does not
    declare returns ``None`` even when a value is stored under it. Agents never
    write to this surface, credentials are provisioned from the desktop app
    (Settings > Integrations) or with ``apollia-os tools credentials set``.
    """

    def get(self, key: str) -> str | None:
        """Returns the secret value, or ``None`` if not configured."""
        ...

    def has(self, key: str) -> bool:
        """Whether ``key`` is declared and currently holds a value.

        Strictly equivalent to ``ctx.secrets.get(key) is not None``.
        """
        ...

    def list_names(self) -> list[str]:
        """Return the secret keys the manifest declares, never their values."""
        ...

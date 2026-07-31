"""ctx.secrets - read-only credentials access."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class SecretsInterface(Protocol):
    """Read-only access to credentials declared in ``@agent(secrets=(...))``.

    Secrets are resolved at task startup by ``apollia-auth`` (keyring +
    OAuth refresh).  Agents never write to this surface - credentials are
    provisioned from the desktop app (Settings > Integrations) or with
    ``apollia-os tools credentials set``.
    """

    def get(self, key: str) -> str | None:
        """Returns the secret value, or ``None`` if not configured."""
        ...

    def has(self, key: str) -> bool: ...

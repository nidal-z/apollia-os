"""ctx.profile - canonical user profile."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class ProfileInterface(Protocol):
    """User profile surface.

    Read-only by default; write methods require the agent manifest to
    declare ``@agent(user_memory_write=True)``.  Calling :meth:`set` or
    :meth:`update` from a non-writable context raises a runtime error.
    """

    @property
    def writable(self) -> bool:
        """Whether this context may write to the profile."""
        ...

    async def get(self, key: str) -> str | None:
        """Return the profile value for ``key``, or None if unset."""
        ...

    async def has(self, key: str) -> bool:
        """Whether ``key`` is set on the profile."""
        ...

    async def all(self) -> dict[str, str]:
        """Return every set profile entry."""
        ...

    def schema_keys(self) -> list[str]:
        """Return the keys the profile schema declares, set or not."""
        ...

    async def set(self, key: str, value: str) -> None:
        """Write a single profile entry.

        Raises:
            RuntimeError: If the context is not writable.
        """
        ...

    async def update(self, entries: dict[str, str]) -> None:
        """Write several profile entries at once.

        Raises:
            RuntimeError: If the context is not writable.
        """
        ...

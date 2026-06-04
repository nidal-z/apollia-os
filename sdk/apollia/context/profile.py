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
    def writable(self) -> bool: ...

    async def get(self, key: str) -> str | None: ...

    async def has(self, key: str) -> bool: ...

    async def all(self) -> dict[str, str]: ...

    def schema_keys(self) -> list[str]: ...

    async def set(self, key: str, value: str) -> None: ...

    async def update(self, entries: dict[str, str]) -> None: ...

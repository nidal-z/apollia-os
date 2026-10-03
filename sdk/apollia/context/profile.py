"""ctx.profile - the user profile the current execution runs with."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class ProfileInterface(Protocol):
    """User profile surface.

    The user may keep several profiles ("IT manager", "trainer"), one of
    them the default. ``ctx.profile`` serves the profile resolved for the
    execution: the one the user designated for this agent if it still exists,
    otherwise the default profile. The resolution happens at each call, so a
    designation changed while the agent runs applies to its next read.

    A profile carries five fields and a free text, all optional:

    * ``name``: the name of the person;
    * ``role``: their role;
    * ``function``: their function;
    * ``tools``: the tools they work with;
    * ``preferences``: how they like to work;
    * ``free_text``: anything else, written freely.

    :meth:`schema_keys` returns these six keys. Any other key reads and
    writes a free-form entry, shared by every profile. The ``user.`` prefix
    of the historical ``remember_user("user.X")`` keys is accepted and
    stripped.

    Read-only by default; write methods require the agent manifest to
    declare ``@agent(user_memory_write=True)``.  Calling :meth:`set` or
    :meth:`update` from a non-writable context raises a runtime error. A
    field written goes to the resolved profile.
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
        """Return the keys the profile schema declares, set or not.

        The five fields (``name``, ``role``, ``function``, ``tools``,
        ``preferences``) then the free text (``free_text``).
        """
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

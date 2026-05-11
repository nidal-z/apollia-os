"""Type stubs for ProfileInterface — Python proxy to the global user profile.

Mirrors the ``#[pyclass]`` / ``#[pymethods]`` definition in
``crates/apollia-aip/src/profile.rs``.  Exists solely for IDE autocompletion
and ``mypy`` validation — the runtime implementation is in Rust and is
injected via PyO3.
"""

from __future__ import annotations

from typing import Awaitable


class ProfileInterface:
    """Global user profile proxy exposed via ``ctx.profile``.

    Provides typed access to the canonical user profile fields declared in the
    Rust schema (``PROFILE_SCHEMA``).  Reads are always allowed; writes
    require ``user_memory_write = true`` in the agent manifest.

    The historical ``user.`` prefix on keys is silently stripped: both
    ``await ctx.profile.set("user.name", "Nidal")`` and
    ``await ctx.profile.set("name", "Nidal")`` write the same flat key.
    """

    @property
    def writable(self) -> bool:
        """``True`` when this agent can write to the user profile."""
        ...

    def get(self, key: str) -> Awaitable[str | None]:
        """Returns the value of a profile field by key, or ``None`` if absent.

        Always available, regardless of the manifest ``user_memory_write`` flag.
        """
        ...

    def has(self, key: str) -> Awaitable[bool]:
        """Returns ``True`` when the key is present in the profile."""
        ...

    def all(self) -> Awaitable[dict[str, str]]:
        """Returns a dict mapping every profile key to its value.

        Internal markers (onboarding bookkeeping) are excluded.
        """
        ...

    def schema_keys(self) -> list[str]:
        """Returns the canonical schema keys defined by ``PROFILE_SCHEMA``.

        Synchronous: the schema is compiled into the binary.
        """
        ...

    def set(self, key: str, value: str) -> Awaitable[None]:
        """Sets or updates a profile field.

        Raises ``RuntimeError`` when the agent manifest does not declare
        ``user_memory_write = true``.

        The ``user.`` prefix on ``key`` is silently stripped to preserve
        compatibility with legacy ``remember_user("user.X")`` call sites.
        """
        ...

    def update(self, entries: dict[str, str]) -> Awaitable[None]:
        """Sets or updates multiple profile fields in one round-trip.

        Same permission rules as :meth:`set`.
        """
        ...

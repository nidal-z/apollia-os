"""Type stubs for MemoryInterface — Python proxy to agent memory.

This class mirrors the ``#[pyclass]`` / ``#[pymethods]`` definition in
``crates/apollia-aip/src/memory.rs``.  It exists purely to provide IDE
autocompletion and ``mypy`` validation — the real implementation lives in
Rust and is injected at runtime via PyO3.
"""

from __future__ import annotations

from datetime import timedelta
from typing import Awaitable


class MemoryInterface:
    """Agent memory proxy exposed via ``ctx.memory``.

    Provides episodic recording, semantic key/value storage,
    full-text search, and deletion.  Write operations are only
    allowed on the agent's primary namespace.
    """

    def record(
        self,
        content: str,
        importance: float | None = None,
        task_id: str | None = None,
        metadata: dict | None = None,
        expires_in: timedelta | None = None,
    ) -> Awaitable[str]:
        """Record an episodic memory event.

        ``importance`` defaults to 0.5 if not provided.
        ``metadata`` is an arbitrary dict stored alongside the entry.
        ``expires_in`` sets a TTL; ``None`` means no expiration.
        Returns the entry ID.
        """
        ...

    def remember(
        self,
        key: str,
        value: str,
        source: str | None = None,
        confidence: float | None = None,
    ) -> Awaitable[None]:
        """Store a key/value pair in semantic memory.

        ``confidence`` is a score between 0.0 and 1.0 (default 1.0).
        When provided, an existing entry with strictly higher confidence
        is preserved (no overwrite).
        """
        ...

    def remember_user(
        self,
        key: str,
        value: str,
        source: str | None = None,
        confidence: float | None = None,
    ) -> Awaitable[None]:
        """Store a key/value pair in the global ``__user__`` namespace.

        Available only to agents whose manifest declares
        ``user_memory_write = true`` (typically the onboarding agent).
        Raises ``RuntimeError`` otherwise.

        .. deprecated:: ADR-087
            Prefer ``ctx.profile.set(key, value)`` — the new canonical surface
            for the user profile.  ``remember_user`` keeps working as a thin
            wrapper that strips the historical ``user.`` prefix and ignores
            the ``source`` / ``confidence`` arguments.

        Once written, the value is readable by every other agent through
        the standard :meth:`recall` fallback.
        """
        ...

    def recall(self, key: str) -> Awaitable[str | None]:
        """Retrieve a value by key from semantic memory.

        Searches the agent's own namespace first, then falls back to the
        global ``__user__`` namespace. Returns ``None`` if the key does
        not exist in either location.
        """
        ...

    def search(
        self,
        query: str,
        limit: int | None = None,
    ) -> Awaitable[list[dict[str, object]]]:
        """Full-text search across agent memory.

        Returns dicts with ``content``, ``score``, ``source``, and
        ``timestamp`` keys.  ``limit`` defaults to 10 if not provided.
        """
        ...

    def recall_entry(
        self,
        key: str,
        injection_reason: str | None = None,
    ) -> Awaitable[dict[str, object] | None]:
        """Retrieve a semantic entry with full metadata.

        Returns a dict with keys: key, value, confidence, source,
        updated_at, expires_at. Returns None if the key does not exist
        or is expired.

        ``injection_reason`` is surfaced to the operator UI (memory injection
        visibility). When omitted, the
        runtime falls back to ``"Matched query: <key>"``.
        """
        ...

    def recall_all(
        self,
        limit: int | None = None,
        injection_reason: str | None = None,
    ) -> Awaitable[list[dict[str, object]]]:
        """List all semantic entries in the agent namespace.

        Returns dicts with the same structure as recall_entry().
        limit defaults to 100.

        ``injection_reason`` applies to *every* entry surfaced by this
        call; provide a coarse rationale (e.g. ``"session bootstrap"``).
        """
        ...

    def recall_procedure(self, trigger: str) -> Awaitable[list[dict]]:
        """Retrieve procedural memory entries matching ``trigger``.

        Returns a list of dicts with keys: ``trigger``, ``steps``, ``created_at``.
        Returns ``[]`` if no procedure matches.
        """
        ...

    def learn_procedure(
        self,
        trigger: str,
        steps: list[str],
    ) -> Awaitable[str]:
        """Record a procedure in procedural memory.

        ``trigger`` is the exact match key used later by ``recall_procedure()``.
        ``steps`` is a non-empty ordered list of procedure steps.
        Raises ``ValueError`` if ``steps`` is empty.
        Returns the UUID of the stored procedure.
        """
        ...

    def forget(self, key: str) -> Awaitable[None]:
        """Remove a key/value pair from semantic memory."""
        ...

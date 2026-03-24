"""Type stubs for MemoryInterface — Python proxy to agent memory.

This class mirrors the ``#[pyclass]`` / ``#[pymethods]`` definition in
``crates/apollia-aip/src/memory.rs``.  It exists purely to provide IDE
autocompletion and ``mypy`` validation — the real implementation lives in
Rust and is injected at runtime via PyO3.
"""

from __future__ import annotations

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
    ) -> Awaitable[None]:
        """Record an episodic memory event.

        ``importance`` defaults to 0.5 if not provided.
        """
        ...

    def remember(
        self,
        key: str,
        value: str,
        source: str | None = None,
    ) -> Awaitable[None]:
        """Store a key/value pair in semantic memory."""
        ...

    def recall(self, key: str) -> Awaitable[str | None]:
        """Retrieve a value by key from semantic memory.

        Returns ``None`` if the key does not exist.
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

    def forget(self, key: str) -> Awaitable[None]:
        """Remove a key/value pair from semantic memory."""
        ...

"""Context Bootstrapping Protocol for Apollia OS agents.

Defines a minimal abstract protocol that any agent can implement to:
1. Explore its domain of work on the first session
2. Persist discovered knowledge in semantic memory
3. Detect staleness and re-bootstrap incrementally

The protocol is domain-agnostic: each agent specialises ``is_stale()``
and ``run_bootstrap()`` according to its domain (dev, document, legal,
accounting, etc.).  The base class handles lifecycle plumbing.

Usage::

    class MyBootstrap(ContextBootstrap):
        async def is_stale(self, ctx):
            meta = await self.load_meta(ctx)
            ...
            return True

        async def run_bootstrap(self, ctx):
            snapshot = {"key": "value", ...}
            await self.persist(ctx, snapshot, staleness_marker="abc123")
            return snapshot

Typical integration in ``run()``::

    async def run(self, task, ctx):
        if await self._bootstrap.needs_bootstrap(ctx):
            await self._bootstrap.run_bootstrap(ctx)
        snapshot = await self._bootstrap.load_snapshot(ctx)
        # ... use snapshot ...
"""

from __future__ import annotations

import json
import time
from abc import ABC, abstractmethod
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from apollia.stubs.memory import MemoryInterface


class ContextBootstrap(ABC):
    """Minimal abstract protocol for context bootstrapping.

    Subclasses MUST implement exactly two methods:

    * ``is_stale(ctx)`` — domain-specific staleness check
    * ``run_bootstrap(ctx)`` — domain-specific exploration and snapshot creation

    Everything else (``needs_bootstrap``, ``load_snapshot``, ``load_meta``,
    ``persist``) is infrastructure with sensible defaults.
    """

    # Standardised memory keys — override per-agent if needed.
    SNAPSHOT_KEY: str = "bootstrap.snapshot"
    META_KEY: str = "bootstrap.meta"
    STATUS_KEY: str = "bootstrap.status"

    # ------------------------------------------------------------------
    # Abstract — the only two methods a subclass must implement
    # ------------------------------------------------------------------

    @abstractmethod
    async def is_stale(self, ctx: Any) -> bool:
        """Return True if the existing snapshot is outdated.

        The definition of "stale" is domain-specific:

        * Dev agent: compare stored commit hash with ``git rev-parse HEAD``
        * Document agent: compare stored timestamp with max mtime in workspace
        * Accounting agent: check fiscal year rollover

        When in doubt, return ``True`` (re-bootstrap is idempotent).
        """
        ...

    @abstractmethod
    async def run_bootstrap(self, ctx: Any) -> dict[str, Any]:
        """Explore the domain and populate semantic memory.

        Implementations should:

        1. Explore via ``ctx.tools.call()`` (file_read, bash_executor, etc.)
        2. Build a ``dict`` snapshot with discovered information
        3. Call ``await self.persist(ctx, snapshot, staleness_marker=...)``
        4. Return the snapshot

        If exploration is only partial, call
        ``await self.persist(ctx, snapshot, ..., status="partial")``.
        A partial persist never overwrites a complete snapshot.

        This method MUST be idempotent: calling it twice produces the
        same memory state.
        """
        ...

    # ------------------------------------------------------------------
    # Infrastructure — sensible defaults, override only if needed
    # ------------------------------------------------------------------

    async def needs_bootstrap(self, ctx: Any) -> bool:
        """Check whether a bootstrap is needed.

        Returns ``True`` when:
        - ``ctx.memory`` is ``None`` (graceful degradation)
        - No status key exists (first session)
        - Status is ``"missing"`` or ``"partial"``
        - Status is ``"complete"`` but ``is_stale()`` returns ``True``
        """
        if ctx.memory is None:
            return True
        status = await ctx.memory.recall(self.STATUS_KEY)
        if status is None or status in ("missing", "partial"):
            return True
        return await self.is_stale(ctx)

    async def load_snapshot(self, ctx: Any) -> dict[str, Any] | None:
        """Load the snapshot from semantic memory.

        Returns ``None`` if no memory, no snapshot key, or invalid JSON.
        """
        if ctx.memory is None:
            return None
        raw = await ctx.memory.recall(self.SNAPSHOT_KEY)
        if raw is None:
            return None
        try:
            return json.loads(raw)
        except (json.JSONDecodeError, TypeError):
            return None

    async def load_meta(self, ctx: Any) -> dict[str, Any] | None:
        """Load bootstrap metadata (version, timestamp, staleness marker).

        Returns ``None`` if no memory or no meta key.
        """
        if ctx.memory is None:
            return None
        raw = await ctx.memory.recall(self.META_KEY)
        if raw is None:
            return None
        try:
            return json.loads(raw)
        except (json.JSONDecodeError, TypeError):
            return None

    async def persist(
        self,
        ctx: Any,
        snapshot: dict[str, Any],
        *,
        staleness_marker: str,
        source: str = "bootstrap",
        status: str = "complete",
        confidence: float = 0.9,
    ) -> None:
        """Persist a snapshot, its metadata, and status to semantic memory.

        Parameters
        ----------
        ctx
            RuntimeContext with ``ctx.memory``.
        snapshot
            The domain-specific snapshot dict to persist.
        staleness_marker
            Opaque string used by ``is_stale()`` to detect changes
            (e.g. a git commit hash, a timestamp, a schema version).
        source
            Memory source tag (default ``"bootstrap"``).
        status
            One of ``"complete"``, ``"partial"``, ``"missing"``.
            A ``"partial"`` persist will NOT overwrite an existing
            ``"complete"`` snapshot.
        confidence
            Confidence score for the snapshot entry (0.0–1.0).
        """
        if ctx.memory is None:
            return

        # Guard: never downgrade a complete snapshot with a partial one
        if status == "partial":
            existing = await ctx.memory.recall(self.STATUS_KEY)
            if existing == "complete":
                return

        meta = {
            "version": 1,
            "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "staleness_marker": staleness_marker,
        }

        await ctx.memory.remember(
            key=self.SNAPSHOT_KEY,
            value=json.dumps(snapshot, ensure_ascii=False),
            source=source,
            confidence=confidence,
        )
        await ctx.memory.remember(
            key=self.META_KEY,
            value=json.dumps(meta),
            source=source,
            confidence=1.0,
        )
        await ctx.memory.remember(
            key=self.STATUS_KEY,
            value=status,
            source=source,
            confidence=1.0,
        )

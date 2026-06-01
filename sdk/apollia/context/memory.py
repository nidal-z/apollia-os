"""ctx.memory - episodic + semantic + procedural memory."""

from __future__ import annotations

from datetime import timedelta
from typing import Any, Protocol, runtime_checkable


@runtime_checkable
class MemoryInterface(Protocol):
    """Tri-mode memory: episodic events, semantic key-value, procedural triggers."""

    # ── Episodic ────────────────────────────────────────────────────────
    async def record(
        self,
        content: str,
        *,
        importance: float = 0.5,
        task_id: str | None = None,
        metadata: dict[str, Any] | None = None,
        expires_in: timedelta | None = None,
    ) -> str: ...

    # ── Semantic ────────────────────────────────────────────────────────
    async def remember(
        self,
        key: str,
        value: str,
        *,
        source: str | None = None,
        confidence: float = 1.0,
    ) -> None: ...

    async def recall(self, key: str) -> str | None: ...

    async def recall_entry(
        self,
        key: str,
        *,
        injection_reason: str | None = None,
    ) -> dict[str, Any] | None: ...

    async def recall_all(
        self,
        *,
        limit: int = 100,
        injection_reason: str | None = None,
    ) -> list[dict[str, Any]]: ...

    async def forget(self, key: str) -> None: ...

    # ── FTS5 search ─────────────────────────────────────────────────────
    async def search(self, query: str, *, limit: int = 10) -> list[dict[str, Any]]: ...

    # ── Procedural ──────────────────────────────────────────────────────
    async def learn_procedure(
        self,
        trigger: str,
        steps: list[str],
    ) -> str: ...

    async def recall_procedure(self, trigger: str) -> list[dict[str, Any]]: ...

    # ── Export / Import (ADR-066) ─────────────────────────────
    async def export(self) -> dict[str, Any]: ...

    async def import_data(self, data: dict[str, Any]) -> None: ...

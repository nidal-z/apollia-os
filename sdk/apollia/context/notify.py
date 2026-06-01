"""ctx.notify - multi-channel notifications."""

from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class NotifyInterface(Protocol):
    """Notification surface (desktop, webhook, future channels)."""

    async def publish(
        self,
        message: str,
        *,
        severity: str = "info",
        title: str | None = None,
        channel: str | None = None,
    ) -> None: ...

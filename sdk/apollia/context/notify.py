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
    ) -> None:
        """Publish a notification to the user.

        Args:
            message: Body of the notification.
            severity: One of ``info``, ``warning`` or ``error``.
            title: Short headline, or None to let the channel derive one.
            channel: Channel to publish on, or None for the configured default.
        """
        ...

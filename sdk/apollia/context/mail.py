"""ctx.mail - asynchronous inter-agent messaging.

Distinct from ``ctx.a2a`` (synchronous skill RPC): ``send`` posts a message into
another agent's durable inbox and returns immediately, while the recipient pulls
it at its own initiative. Delivery is at-least-once: ``receive`` leases a message
rather than deleting it; the consuming context acknowledges it on success, and
``ack``/``nack`` are available for explicit control.

This module defines a ``TypedDict``, so it intentionally does not use
``from __future__ import annotations`` (PEP 563 would stringify the annotations
and break ``TypedDict.__required_keys__`` that AgentKit reads at runtime).
"""

from typing import Protocol, TypedDict, runtime_checkable


class MailMessage(TypedDict):
    """A message pulled from an agent's durable inbox."""

    message_id: str
    from_agent: str
    payload: dict
    sent_at: str  # RFC3339


@runtime_checkable
class MailInterface(Protocol):
    """Durable, at-least-once inter-agent messaging surface."""

    async def send(self, to: str, payload: dict) -> str:
        """Post a message to ``to``'s inbox, returning its message id."""
        ...

    async def receive(self, timeout_secs: float | None = None) -> MailMessage | None:
        """Lease the next message, waiting up to ``timeout_secs`` seconds."""
        ...

    async def poll(self) -> MailMessage | None:
        """Non-blocking receive: the next leased message or ``None``."""
        ...

    async def pending(self) -> int:
        """Number of non-expired messages in the caller's inbox."""
        ...

    async def list(self, limit: int = 50) -> list[MailMessage]:
        """List up to ``limit`` messages (most recent first) without consuming."""
        ...

    async def ack(self, message_id: str) -> None:
        """Acknowledge a leased message, deleting it from the store."""
        ...

    async def nack(self, message_id: str) -> None:
        """Refuse a leased message, making it deliverable again."""
        ...

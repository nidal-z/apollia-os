---
sidebar_position: 5
title: ctx.mail
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.mail`

Service type: `MailInterface` (from `apollia.context.mail`).

### `MailInterface`

_Bases: Protocol_

Durable, at-least-once inter-agent messaging surface.

#### `send`

```python
async def send(self, to: str, payload: dict[str, Any]) -> str
```

Post a message to ``to``'s inbox, returning its message id.

#### `receive`

```python
async def receive(self, timeout_secs: float | None=None) -> MailMessage | None
```

Lease the next message, waiting up to ``timeout_secs`` seconds.

#### `poll`

```python
async def poll(self) -> MailMessage | None
```

Non-blocking receive: the next leased message or ``None``.

#### `pending`

```python
async def pending(self) -> int
```

Number of non-expired messages in the caller's inbox.

#### `list`

```python
async def list(self, limit: int=50) -> list[MailMessage]
```

List up to ``limit`` messages (most recent first) without consuming.

#### `ack`

```python
async def ack(self, message_id: str) -> None
```

Acknowledge a leased message, deleting it from the store.

#### `nack`

```python
async def nack(self, message_id: str) -> None
```

Refuse a leased message, making it deliverable again.

### `MailMessage`

_Bases: TypedDict_

A message pulled from an agent's durable inbox.

| Field | Type | Default |
| --- | --- | --- |
| `message_id` | `str` |  |
| `from_agent` | `str` |  |
| `payload` | `dict[str, Any]` |  |
| `sent_at` | `str` |  |

---
sidebar_position: 14
title: ctx.notify
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.notify`

Service type: `NotifyInterface` (from `apollia.context.notify`).

### `NotifyInterface`

_Bases: Protocol_

Notification surface (desktop, webhook, future channels).

#### `publish`

```python
async def publish(self, message: str, *, severity: str='info', title: str | None=None, channel: str | None=None) -> None
```

Publish a notification to the user.

Args:
    message: Body of the notification.
    severity: One of ``info``, ``warning`` or ``error``.
    title: Short headline, or None to let the channel derive one.
    channel: Channel to publish on, or None for the configured default.

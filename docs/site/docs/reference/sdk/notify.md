---
sidebar_position: 13
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

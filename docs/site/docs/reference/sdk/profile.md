---
sidebar_position: 11
title: ctx.profile
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.profile`

Service type: `ProfileInterface` (from `apollia.context.profile`).

### `ProfileInterface`

_Bases: Protocol_

User profile surface.

Read-only by default; write methods require the agent manifest to
declare ``@agent(user_memory_write=True)``.  Calling :meth:`set` or
:meth:`update` from a non-writable context raises a runtime error.

| Field | Type | Default |
| --- | --- | --- |
| `writable` | `bool` |  |

#### `get`

```python
async def get(self, key: str) -> str | None
```

#### `has`

```python
async def has(self, key: str) -> bool
```

#### `all`

```python
async def all(self) -> dict[str, str]
```

#### `schema_keys`

```python
def schema_keys(self) -> list[str]
```

#### `set`

```python
async def set(self, key: str, value: str) -> None
```

#### `update`

```python
async def update(self, entries: dict[str, str]) -> None
```

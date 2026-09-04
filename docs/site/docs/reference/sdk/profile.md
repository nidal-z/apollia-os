---
sidebar_position: 11
title: ctx.profile
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.profile`

Service type: `ProfileInterface | None` (from `apollia.context.profile`).

The bridge may leave this service unattached; `ctx.profile` is then `None`.

### `ProfileInterface`

_Bases: Protocol_

User profile surface.

Read-only by default; write methods require the agent manifest to
declare ``@agent(user_memory_write=True)``.  Calling `set` or
`update` from a non-writable context raises a runtime error.

| Field | Type | Default |
| --- | --- | --- |
| `writable` | `bool` |  |

#### `get`

```python
async def get(self, key: str) -> str | None
```

Return the profile value for ``key``, or None if unset.

#### `has`

```python
async def has(self, key: str) -> bool
```

Whether ``key`` is set on the profile.

#### `all`

```python
async def all(self) -> dict[str, str]
```

Return every set profile entry.

#### `schema_keys`

```python
def schema_keys(self) -> list[str]
```

Return the keys the profile schema declares, set or not.

#### `set`

```python
async def set(self, key: str, value: str) -> None
```

Write a single profile entry.

Raises:
    RuntimeError: If the context is not writable.

#### `update`

```python
async def update(self, entries: dict[str, str]) -> None
```

Write several profile entries at once.

Raises:
    RuntimeError: If the context is not writable.

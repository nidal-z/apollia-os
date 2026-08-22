---
sidebar_position: 2
title: ctx.memory
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.memory`

Service type: `MemoryInterface` (from `apollia.context.memory`).

The bridge may leave this service unattached; `ctx.memory` is then `None`.

### `MemoryInterface`

_Bases: Protocol_

Tri-mode memory: episodic events, semantic key-value, procedural triggers.

#### `record`

```python
async def record(self, content: str, *, importance: float=0.5, task_id: str | None=None, metadata: dict[str, Any] | None=None, expires_in: timedelta | None=None) -> str
```

Append an episodic event and return its identifier.

Args:
    content: What happened, in the agent's own words.
    importance: Retention weight in ``[0.0, 1.0]``; low-importance
        events are evicted first.
    task_id: Run this event belongs to, when it belongs to one.
    metadata: Free-form structured payload stored alongside the event.
    expires_in: Time-to-live after which the event is dropped.

Returns:
    The identifier of the recorded event.

#### `remember`

```python
async def remember(self, key: str, value: str, *, source: str | None=None, confidence: float=1.0) -> None
```

Store a semantic fact, overwriting any previous value for ``key``.

Args:
    key: Stable identifier for the fact.
    value: The fact itself.
    source: Where the fact came from, for later auditing.
    confidence: How much the agent trusts the fact, in ``[0.0, 1.0]``.

#### `recall`

```python
async def recall(self, key: str) -> str | None
```

Return the semantic value stored under ``key``, or None if absent.

#### `recall_entry`

```python
async def recall_entry(self, key: str, *, injection_reason: str | None=None) -> dict[str, Any] | None
```

Return the full semantic entry for ``key``, metadata included.

Args:
    key: Identifier of the fact to read.
    injection_reason: Why the agent is reading it, recorded in the
        audit journal. Memory is read at agent initiative, so the
        reason is what makes that initiative reviewable.

Returns:
    The entry with its value, source and confidence, or None if absent.

#### `recall_all`

```python
async def recall_all(self, *, limit: int=100, injection_reason: str | None=None) -> list[dict[str, Any]]
```

Return every semantic entry, most recently written first.

Args:
    limit: Maximum number of entries to return.
    injection_reason: Why the agent is reading them, recorded in the
        audit journal.

Returns:
    The entries, each with its value, source and confidence.

#### `forget`

```python
async def forget(self, key: str) -> None
```

Drop the semantic fact stored under ``key``, if any.

#### `search`

```python
async def search(self, query: str, *, limit: int=10) -> list[dict[str, Any]]
```

Full-text search across episodic and semantic memory.

Args:
    query: FTS5 query string.
    limit: Maximum number of matches to return.

Returns:
    The matching entries, best match first.

#### `learn_procedure`

```python
async def learn_procedure(self, trigger: str, steps: list[str]) -> str
```

Record a reusable sequence of steps under ``trigger``.

Args:
    trigger: Condition the agent will later match against.
    steps: Ordered steps to replay when the trigger matches.

Returns:
    The identifier of the stored procedure.

#### `recall_procedure`

```python
async def recall_procedure(self, trigger: str) -> list[dict[str, Any]]
```

Return the procedures whose trigger matches ``trigger``.

#### `export`

```python
async def export(self) -> dict[str, Any]
```

Return the whole memory as a serialisable snapshot.

#### `import_data`

```python
async def import_data(self, data: dict[str, Any], *, replace: bool=False) -> int
```

Merge a snapshot produced by :meth:`export` into this memory.

``data`` follows the ``format_version = 1`` schema. The default mode is
a merge: an entry whose ``id`` already exists is left alone. Pass
``replace=True`` to clear the namespace first. Returns the number of
entries actually imported.

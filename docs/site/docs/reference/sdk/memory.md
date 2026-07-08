---
sidebar_position: 2
title: ctx.memory
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.memory`

Service type: `MemoryInterface` (from `apollia.context.memory`).

### `MemoryInterface`

_Bases: Protocol_

Tri-mode memory: episodic events, semantic key-value, procedural triggers.

#### `record`

```python
async def record(self, content: str, *, importance: float=0.5, task_id: str | None=None, metadata: dict[str, Any] | None=None, expires_in: timedelta | None=None) -> str
```

#### `remember`

```python
async def remember(self, key: str, value: str, *, source: str | None=None, confidence: float=1.0) -> None
```

#### `recall`

```python
async def recall(self, key: str) -> str | None
```

#### `recall_entry`

```python
async def recall_entry(self, key: str, *, injection_reason: str | None=None) -> dict[str, Any] | None
```

#### `recall_all`

```python
async def recall_all(self, *, limit: int=100, injection_reason: str | None=None) -> list[dict[str, Any]]
```

#### `forget`

```python
async def forget(self, key: str) -> None
```

#### `search`

```python
async def search(self, query: str, *, limit: int=10) -> list[dict[str, Any]]
```

#### `learn_procedure`

```python
async def learn_procedure(self, trigger: str, steps: list[str]) -> str
```

#### `recall_procedure`

```python
async def recall_procedure(self, trigger: str) -> list[dict[str, Any]]
```

#### `export`

```python
async def export(self) -> dict[str, Any]
```

#### `import_data`

```python
async def import_data(self, data: dict[str, Any]) -> None
```

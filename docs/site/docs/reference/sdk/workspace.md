---
sidebar_position: 11
title: ctx.workspace
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.workspace`

Service type: `WorkspaceContext` (from `apollia.context.workspace`).

### `WorkspaceContext`

_Bases: Protocol_

Snapshot of the workspace at task start.

Exposes the parsed ``APOLLIA.md`` (project rules) and named sections.
The snapshot is immutable for the duration of the task.

#### `rules`

```python
def rules(self) -> str | None
```

APOLLIA.md content (alias for :attr:`apollia_md`).

#### `apollia_md`

```python
def apollia_md(self) -> str | None
```

#### `get`

```python
def get(self, title: str) -> str | None
```

Custom section by title.

#### `sections`

```python
def sections(self) -> list[dict[str, str]]
```

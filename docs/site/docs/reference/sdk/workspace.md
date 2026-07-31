---
sidebar_position: 12
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

| Field | Type | Default |
| --- | --- | --- |
| `rules` | `str | None` |  |
| `apollia_md` | `str | None` |  |
| `sections` | `list[dict[str, str]]` |  |

#### `get`

```python
def get(self, title: str) -> str | None
```

Custom section by title.

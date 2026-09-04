---
sidebar_position: 12
title: ctx.workspace
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.workspace`

Service type: `WorkspaceContext` (from `apollia.context.workspace`).

The bridge never attaches this service. `ctx.workspace` is `None` on every binary this project ships, so any attribute access on it raises `AttributeError`; no builder that could fill it (`with_empty_workspace`, `with_workspace_snapshot`) has a caller outside tests. `scripts/check_optional_builders.py` holds that measurement.

### `WorkspaceContext`

_Bases: Protocol_

Snapshot of the workspace at task start.

Exposes the parsed ``APOLLIA.md`` (project rules) and named sections.
The snapshot is immutable for the duration of the task.

| Field | Type | Default |
| --- | --- | --- |
| `rules` | `str \| None` |  |
| `apollia_md` | `str \| None` |  |
| `sections` | `list[dict[str, str]]` |  |

#### `get`

```python
def get(self, title: str) -> str | None
```

Custom section by title.

---
sidebar_position: 11
title: ctx.workspace
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.workspace`

Service type: `WorkspaceContext` (from `apollia.context.workspace`).

The bridge never attaches this service. `ctx.workspace` is `None` on every binary this project ships, so any attribute access on it raises `AttributeError`; no builder that could fill it (`with_empty_workspace`, `with_workspace_snapshot`) has a caller outside tests. `scripts/check_optional_builders.py` holds that measurement.

### `WorkspaceContext`

_Bases: Protocol_

The sections collected from a workspace, by title.

The rules file of a workspace (``APOLLIA.md`` by default) is the section
titled ``Project rules``; other sections carry their own title. The
runtime collects no workspace for the code of a package, so this view is
never attached and ``ctx.workspace`` is ``None``.

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

---
sidebar_position: 6
title: ctx.datasources
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.datasources`

Service type: `DatasourcesInterface` (from `apollia.context.datasources`).

### `DatasourcesInterface`

_Bases: Protocol_

Runtime access to YAML datasources declared in ``@agent(datasources=(...))``.

#### `get`

```python
def get(self, name: str) -> Any
```

Load datasource by name.

Returns parsed YAML (``dict`` / ``list`` / ``str`` / ``int`` / ...).
Raises :class:`FileNotFoundError` if ``name`` is not declared in the
agent manifest.

#### `list_names`

```python
def list_names(self) -> list[str]
```

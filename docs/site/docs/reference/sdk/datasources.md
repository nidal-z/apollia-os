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
Raises `FileNotFoundError` if ``name`` is not declared in the
agent manifest.

#### `has`

```python
def has(self, name: str) -> bool
```

Whether ``name`` is declared and loaded.

More idiomatic than wrapping `get` in a ``try``/``except`` when
the agent means to degrade gracefully on a missing YAML file.

#### `list_names`

```python
def list_names(self) -> list[str]
```

Return the names of every datasource the manifest declares.

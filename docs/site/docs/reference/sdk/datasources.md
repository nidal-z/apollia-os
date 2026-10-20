---
sidebar_position: 5
title: ctx.datasources
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.datasources`

Service type: `DatasourcesInterface` (from `apollia.context.datasources`).

### `DatasourcesInterface`

_Bases: Protocol_

Runtime access to the YAML datasources the context was given.

A datasource is a YAML file the context was given by name. No datasource
is given today: ``apollia.toml`` has no section that names one, and the
runtime builds this service empty, so on every binary this project ships
``get`` raises `FileNotFoundError`, ``has`` returns ``False`` and
``list_names`` returns an empty list.

#### `get`

```python
def get(self, name: str) -> Any
```

Load datasource by name.

Returns parsed YAML (``dict`` / ``list`` / ``str`` / ``int`` / ...).
Raises `FileNotFoundError` if ``name`` was not given to the
context, or was given but its YAML file could not be loaded.

#### `has`

```python
def has(self, name: str) -> bool
```

Whether ``name`` was given to the context and loaded.

More idiomatic than wrapping `get` in a ``try``/``except`` when
the agent means to degrade gracefully on a missing YAML file.

#### `list_names`

```python
def list_names(self) -> list[str]
```

Return the names of every datasource the context was given.

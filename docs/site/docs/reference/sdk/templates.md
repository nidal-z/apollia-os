---
sidebar_position: 6
title: ctx.templates
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.templates`

Service type: `TemplatesInterface` (from `apollia.context.templates`).

### `TemplatesInterface`

_Bases: Protocol_

Runtime Jinja2 template rendering.

A template is one the context was given, addressed by its name. No
template is given today: ``apollia.toml`` has no section that names one,
and the runtime builds this service empty, so on every binary this project
ships ``render`` raises `FileNotFoundError`, ``has`` returns
``False`` and ``list_names`` returns an empty list.

#### `render`

```python
def render(self, name: str, **context: object) -> str
```

Render a template the context was given.

Args:
    name: Name of a template the context was given.
    **context: Variables exposed to the template.

Returns:
    The rendered text.

Raises:
    FileNotFoundError: ``name`` was not given to the context, or its
        file could not be compiled.

#### `has`

```python
def has(self, name: str) -> bool
```

Whether ``name`` was given to the context and compiled in memory.

#### `list_names`

```python
def list_names(self) -> list[str]
```

Return the names of every template the context was given.

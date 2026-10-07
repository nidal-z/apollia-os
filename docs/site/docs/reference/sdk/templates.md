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

A template is one the context was given, addressed by its name.

#### `render`

```python
def render(self, name: str, **context: object) -> str
```

Render a declared template.

Args:
    name: Name of a template the context was given.
    **context: Variables exposed to the template.

Returns:
    The rendered text.

#### `has`

```python
def has(self, name: str) -> bool
```

Whether ``name`` is declared and compiled in memory.

#### `list_names`

```python
def list_names(self) -> list[str]
```

Return the names of every template the manifest declares.

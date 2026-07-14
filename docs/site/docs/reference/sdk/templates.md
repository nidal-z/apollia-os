---
sidebar_position: 7
title: ctx.templates
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.templates`

Service type: `TemplatesInterface` (from `apollia.context.templates`).

### `TemplatesInterface`

_Bases: Protocol_

Runtime Jinja2 template rendering.

Templates are declared via ``@agent(templates=(...))`` and resolved
from the agent package's ``templates/`` directory at task startup.

#### `render`

```python
def render(self, name: str, **context: Any) -> str
```

#### `list_names`

```python
def list_names(self) -> list[str]
```

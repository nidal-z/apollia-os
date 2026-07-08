---
sidebar_position: 3
title: ctx.tools
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.tools`

Service type: `ToolProxy` (from `apollia.context.tools`).

### `ToolProxy`

_Bases: Protocol_

Tool invocation surface - native registry + MCP routing.

Tools prefixed with ``mcp:<server>/<name>`` are dispatched to a
connected MCP server.  All other tool names resolve through the
native ``apollia-tools`` registry.

#### `call`

```python
async def call(self, tool_name: str, input: dict[str, Any]) -> dict[str, Any]
```

#### `list_tools`

```python
def list_tools(self) -> list[str]
```

#### `describe`

```python
async def describe(self, name: str) -> dict[str, Any] | None
```

#### `tool_call_count`

```python
def tool_call_count(self) -> int
```

### `ToolDescriptor`

_Bases: Protocol_

Metadata describing a registered tool.

| Field | Type | Default |
| --- | --- | --- |
| `name` | `str` |  |
| `version` | `str` |  |
| `description` | `str` |  |
| `input_schema` | `dict[str, Any]` |  |
| `output_schema` | `dict[str, Any]` |  |
| `tags` | `list[str]` |  |

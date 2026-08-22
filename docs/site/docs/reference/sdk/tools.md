---
sidebar_position: 3
title: ctx.tools
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# `ctx.tools`

Service type: `ToolProxy` (from `apollia.context.tools`).

The bridge may leave this service unattached; `ctx.tools` is then `None`.

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

Invoke a tool and return its output.

Args:
    tool_name: Native tool name, or ``mcp:<server>/<name>`` to route
        the call to a connected MCP server.
    input: Payload, validated against the tool input schema.

Returns:
    The tool output, matching its declared output schema.

#### `list_tools`

```python
def list_tools(self) -> list[str]
```

Return the names of every tool reachable from this context.

#### `describe`

```python
async def describe(self, name: str) -> dict[str, Any] | None
```

Return the descriptor for ``name``, or None if no such tool.

#### `tool_call_count`

```python
def tool_call_count(self) -> int
```

Return how many tool calls this run has made so far.

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

"""ctx.tools - native tool invocation + MCP routing."""

from __future__ import annotations

from typing import Any, Protocol, runtime_checkable


@runtime_checkable
class ToolDescriptor(Protocol):
    """Metadata describing a registered tool."""

    name: str
    version: str
    description: str
    input_schema: dict[str, Any]
    output_schema: dict[str, Any]
    tags: list[str]


@runtime_checkable
class ToolProxy(Protocol):
    """Tool invocation surface - native registry + MCP routing.

    Tools prefixed with ``mcp:<server>/<name>`` are dispatched to a
    connected MCP server.  All other tool names resolve through the
    native ``apollia-tools`` registry.
    """

    async def call(
        self,
        tool_name: str,
        input: dict[str, Any],
    ) -> dict[str, Any]: ...

    def list_tools(self) -> list[str]: ...

    async def describe(self, name: str) -> dict[str, Any] | None: ...

    def tool_call_count(self) -> int: ...

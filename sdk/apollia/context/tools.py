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
    ) -> dict[str, Any]:
        """Invoke a tool and return its output.

        Args:
            tool_name: Native tool name, or ``mcp:<server>/<name>`` to route
                the call to a connected MCP server.
            input: Payload, validated against the tool input schema.

        Returns:
            The tool output, matching its declared output schema. An MCP
            tool returns ``{"content": "<text>"}``, every text part joined,
            plus ``"structured"`` holding the object itself when the server
            sends a structured result, so an object never has to be parsed
            back from ``content``.

        Raises:
            NeedHumanInput: The call needs an approval, the server is
                declared ``requires_approval``. Left uncaught, the task
                pauses on it and keeps the context the run was resumed with.
            ToolApprovalDenied: The task was resumed with this call declined.
        """
        ...

    def list_tools(self) -> list[str]:
        """Return the names of every tool reachable from this context."""
        ...

    async def describe(self, name: str) -> dict[str, Any] | None:
        """Return the descriptor for ``name``, or None if no such tool."""
        ...

    def tool_call_count(self) -> int:
        """Return how many tool calls this run has made so far."""
        ...

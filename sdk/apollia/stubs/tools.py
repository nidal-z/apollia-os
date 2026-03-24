"""Type stubs for ToolProxy — Python proxy to Rust tool registry.

These classes mirror the ``#[pyclass]`` / ``#[pymethods]`` definitions in
``crates/apollia-aip/src/context.rs``.  They exist purely to provide IDE
autocompletion and ``mypy`` validation — the real implementations live in
Rust and are injected at runtime via PyO3.
"""

from __future__ import annotations

from typing import Awaitable


class ToolProxy:
    """Proxy exposing Rust-native tools to an agent via ``ctx.tools``.

    Each agent receives its own ``ToolProxy`` configured with its allowed
    tools.  All calls are permission-checked, audited, and counted.
    """

    def call(self, tool_name: str, input: dict[str, object]) -> Awaitable[dict[str, object]]:
        """Invoke a tool by name with a dict payload.

        Returns the tool's JSON output as a Python dict.
        Raises ``RuntimeError`` if the tool is not allowed or not found.
        """
        ...

    def list_tools(self) -> list[str]:
        """Return the names of tools available to this agent."""
        ...

    def tool_call_count(self) -> int:
        """Return the total number of tool calls made so far."""
        ...

    def describe(self, name: str) -> Awaitable[dict[str, object] | None]:
        """Return the JSON schema of a tool, or ``None`` if not registered.

        The returned dict contains keys: ``name``, ``version``,
        ``description``, ``input_schema``, ``output_schema``, ``tags``.
        """
        ...

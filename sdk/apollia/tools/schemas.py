"""Schema descriptions for native Apollia OS tools.

These schemas are injected into the system prompt so the LLM knows
parameter names and types for each built-in tool.  Extend this dict
when adding new native tools to ``apollia-tools``.
"""

from __future__ import annotations

from typing import Any


NATIVE_TOOL_SCHEMAS: dict[str, dict[str, Any]] = {
    "bash_executor": {
        "description": "Execute a shell command and return stdout + stderr.",
        "parameters": {
            "command": "str — the shell command to run",
            "timeout_seconds": "int (optional, default 30)",
        },
        "example": '{"command": "ls -la /tmp", "timeout_seconds": 10}',
    },
    "file_io": {
        "description": (
            "Read, write, list, or check existence of files on the local "
            "filesystem."
        ),
        "parameters": {
            "action": "str — one of: read | write | list | exists",
            "path": "str — absolute or relative path",
            "content": "str (write only) — content to write",
            "pattern": "str (list only) — glob pattern, e.g. '*.rs'",
        },
        "example": '{"action": "read", "path": "/tmp/notes.txt"}',
    },
    "python_executor": {
        "description": "Execute Python 3 code in an isolated venv and return stdout.",
        "parameters": {
            "code": "str — the Python source code to run",
            "timeout_seconds": "int (optional, default 30)",
        },
        "example": '{"code": "import sys; print(sys.version)"}',
    },
}


def describe_tool(tool_name: str) -> str:
    """Return a compact schema string for one tool."""
    schema = NATIVE_TOOL_SCHEMAS.get(tool_name)
    if schema is None:
        return f"  {tool_name}: (no schema available — use empty args {{}} to probe)"

    params = "\n".join(
        f"      {k}: {v}" for k, v in schema["parameters"].items()
    )
    return (
        f"  {tool_name}:\n"
        f"    Description: {schema['description']}\n"
        f"    Parameters:\n{params}\n"
        f"    Example args: {schema['example']}"
    )


def build_tools_block(tool_names: list[str]) -> str:
    """Build the tools section of the system prompt."""
    if not tool_names:
        return "No tools are available — provide a final answer directly."
    return "Available tools:\n\n" + "\n\n".join(
        describe_tool(name) for name in tool_names
    )

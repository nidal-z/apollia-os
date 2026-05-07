"""Tests for the tool-schema rendering layer.

Covers both the legacy synchronous helpers (``NATIVE_TOOL_SCHEMAS``,
``describe_tool``, ``build_tools_block``) and the runtime-driven path
(``build_tools_block_from_ctx``, ``render_descriptor``) that pulls
canonical descriptors from ``ctx.tools.describe()``.
"""

from __future__ import annotations

import asyncio
import json
from typing import Any

import pytest

from apollia.tools.schemas import (
    NATIVE_TOOL_SCHEMAS,
    build_tools_block,
    build_tools_block_from_ctx,
    describe_tool,
    render_descriptor,
)


# ---------------------------------------------------------------------------
# Legacy synchronous path (offline fallback)
# ---------------------------------------------------------------------------


def test_native_tool_schemas_has_known_tools() -> None:
    # GIVEN the static SDK mirror of the Rust descriptors
    # WHEN we look up known native tool names
    # THEN they all have an entry — guards against silent drift from previous bugs.
    expected = {
        "bash_executor",
        "file_read",
        "file_write",
        "file_edit",
        "file_list",
        "file_glob",
        "file_grep",
        "ask_user",
        "memory_search",
        "http_fetch",
        "python_executor",
        "notebook_read",
        "notebook_edit",
        "web_search",
        "web_read",
    }
    missing = expected - set(NATIVE_TOOL_SCHEMAS.keys())
    assert not missing, f"missing legacy schemas: {missing}"


def test_describe_tool_unknown_a2a_skill() -> None:
    # GIVEN an unknown A2A skill name
    # WHEN we describe it through the legacy renderer
    # THEN we get the generic A2A explanation, not a "(no schema available)" hint
    out = describe_tool("a2a:my-custom-skill")
    assert "Delegate a task" in out
    assert "my-custom-skill" in out


def test_build_tools_block_empty() -> None:
    # GIVEN an empty list
    # WHEN we build the prompt block
    # THEN we get the expected "no tools" sentinel
    assert "No tools are available" in build_tools_block([])


# ---------------------------------------------------------------------------
# Runtime-driven path — single source of truth via ctx.tools.describe()
# ---------------------------------------------------------------------------


class _StubToolProxy:
    """Minimal stub mimicking the runtime ToolProxy.describe() coroutine."""

    def __init__(self, descriptors: dict[str, dict[str, Any] | None]) -> None:
        self._descriptors = descriptors

    async def describe(self, name: str) -> dict[str, Any] | None:
        return self._descriptors.get(name)


class _StubCtx:
    def __init__(self, tools: _StubToolProxy | None) -> None:
        self.tools = tools


def test_render_descriptor_from_runtime_input_schema() -> None:
    # GIVEN a descriptor returned by the Rust registry (web_search-shaped)
    descriptor = {
        "name": "web_search",
        "version": "1.0.0",
        "description": "Search the web …",
        "input_schema": {
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 500,
                    "description": "Search terms.",
                },
                "max_results": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 20,
                    "description": "Max results.",
                },
                "backend": {
                    "type": "string",
                    "enum": ["auto", "duckduckgo", "brave"],
                    "description": "Pin a backend.",
                },
            },
            "required": ["query"],
        },
    }

    # WHEN we render it
    out = render_descriptor("web_search", descriptor)

    # THEN the rendering carries everything the LLM needs to call it
    assert "web_search:" in out
    assert "Search the web" in out
    assert "query: str (required, 1-500 chars)" in out
    assert "max_results: int (optional, 1-20)" in out
    assert "backend: str (optional, one of: auto | duckduckgo | brave)" in out
    # Example honours the `required` array — query placeholder must be present
    assert '"query": "<query>"' in out


def test_render_descriptor_falls_back_when_runtime_returns_none() -> None:
    # GIVEN ctx.tools.describe returned None for an A2A skill
    # WHEN we render
    # THEN we degrade to the legacy A2A renderer instead of dropping context
    out = render_descriptor("a2a:my-skill", None)
    assert "Delegate a task" in out


def test_build_tools_block_from_ctx_uses_runtime_descriptors() -> None:
    # GIVEN a stub registry that returns a descriptor with one required string
    descriptors = {
        "echo": {
            "name": "echo",
            "description": "Echoes input back.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "msg": {"type": "string", "description": "Message."}
                },
                "required": ["msg"],
            },
        }
    }
    ctx = _StubCtx(tools=_StubToolProxy(descriptors))

    # WHEN we build the block via the runtime path
    block = asyncio.run(build_tools_block_from_ctx(ctx, ["echo"]))

    # THEN the rendering mirrors the runtime descriptor (not any SDK constant)
    assert "Available tools:" in block
    assert "echo:" in block
    assert "Echoes input back." in block
    assert "msg: str (required) — Message." in block


def test_build_tools_block_from_ctx_degrades_without_ctx() -> None:
    # GIVEN no ctx (offline mode, dry-run, tests without runtime)
    # WHEN we build the block
    # THEN we silently fall back to the static SDK mirror
    block = asyncio.run(build_tools_block_from_ctx(None, ["bash_executor"]))
    assert "bash_executor:" in block
    assert "Execute a shell command" in block


def test_build_tools_block_from_ctx_degrades_when_describe_raises() -> None:
    # GIVEN a stub that raises (simulates runtime registry failure)
    class _BoomProxy:
        async def describe(self, name: str) -> dict[str, Any] | None:
            raise RuntimeError("registry offline")

    ctx = _StubCtx(tools=_BoomProxy())

    # WHEN we build the block
    block = asyncio.run(build_tools_block_from_ctx(ctx, ["bash_executor"]))

    # THEN the failure is swallowed and the legacy renderer takes over
    assert "bash_executor:" in block
    assert "Execute a shell command" in block


def test_build_tools_block_from_ctx_synthesises_example_for_required_only() -> None:
    # GIVEN a tool whose required field carries an enum
    descriptors = {
        "router": {
            "name": "router",
            "description": "Pick a route.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["fast", "thorough"],
                    }
                },
                "required": ["mode"],
            },
        }
    }
    ctx = _StubCtx(tools=_StubToolProxy(descriptors))

    # WHEN we build the block
    block = asyncio.run(build_tools_block_from_ctx(ctx, ["router"]))

    # THEN the synthesised example uses the first enum value (not a placeholder string)
    assert '"mode": "fast"' in block


@pytest.mark.parametrize("tool_names", [[], ["nonexistent_tool"]])
def test_build_tools_block_from_ctx_handles_edge_cases(tool_names: list[str]) -> None:
    # GIVEN edge inputs (empty list / unknown tool with empty registry)
    ctx = _StubCtx(tools=_StubToolProxy({}))
    block = asyncio.run(build_tools_block_from_ctx(ctx, tool_names))
    if not tool_names:
        assert "No tools are available" in block
    else:
        # Unknown tools fall back to the legacy renderer's "no schema" hint.
        assert "no schema available" in block

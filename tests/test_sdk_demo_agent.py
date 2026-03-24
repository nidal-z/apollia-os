"""Tests for sdk-demo-agent using Apollia SDK testing utilities.

Validates that the demonstration agent exercises key SDK features:
tool introspection, memory, HITL, parsing, and Markdown formatting.
"""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
from typing import Any

import pytest

from apollia.testing import MockContext
from apollia.testing.assertions import (
    assert_llm_called,
    assert_result_completed,
    assert_result_input_required,
    assert_tool_called,
)

# ---------------------------------------------------------------------------
# Agent import (filename uses hyphens — requires importlib)
# ---------------------------------------------------------------------------

_PROJECT_ROOT = Path(__file__).resolve().parent.parent
_AGENT_PATH = _PROJECT_ROOT / "agents" / "sdk-demo-agent.py"

_spec = importlib.util.spec_from_file_location("sdk_demo_agent", str(_AGENT_PATH))
if _spec is None or _spec.loader is None:
    pytest.skip(
        f"Agent file not found: {_AGENT_PATH}",
        allow_module_level=True,
    )

_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_mod)
SdkDemoAgent = _mod.SdkDemoAgent


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_task(
    text: str = "Analyze the project structure",
    **overrides: Any,
) -> dict[str, Any]:
    """Build a minimal AIP task dict."""
    task: dict[str, Any] = {
        "task_id": "test-task-001",
        "input": {"parts": [{"type": "text", "text": text}]},
    }
    task.update(overrides)
    return task


_LLM_ANALYSIS_JSON = json.dumps({
    "summary": "A well-structured Rust workspace with 7 crates.",
    "highlights": [
        "Clean separation of concerns",
        "Comprehensive test coverage",
    ],
    "file_count": 42,
    "suggestion": "Add integration benchmarks.",
})


# ---------------------------------------------------------------------------
# Manifest
# ---------------------------------------------------------------------------

class TestManifest:
    """Verify the agent manifest is well-formed."""

    def test_manifest_has_required_fields(self) -> None:
        """GIVEN a SdkDemoAgent WHEN manifest() THEN required fields present."""
        m = SdkDemoAgent().manifest()
        assert m["name"] == "sdk-demo-agent"
        assert "version" in m
        assert "tools_required" in m
        assert m["execution_mode"] == "direct"
        assert isinstance(m.get("step_budget"), dict)

    def test_manifest_declares_bash_executor(self) -> None:
        """GIVEN a SdkDemoAgent WHEN manifest() THEN bash_executor required."""
        m = SdkDemoAgent().manifest()
        assert "bash_executor" in m["tools_required"]


# ---------------------------------------------------------------------------
# Full run
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestRunCompletion:
    """Verify the agent completes with tools, LLM, and memory."""

    async def test_runs_to_completion(self) -> None:
        """GIVEN tools + LLM + memory WHEN run() THEN completed + tools called."""
        ctx = MockContext.create(
            tools={
                "bash_executor": {
                    "stdout": "total 7\ndrwxr-xr-x crates/\n-rw-r--r-- Cargo.toml",
                },
                "file_io": {"status": "ok"},
            },
            llm_responses=[{"content": _LLM_ANALYSIS_JSON}],
            memory=True,
        )
        result = await SdkDemoAgent().run(_make_task(), ctx)

        assert_result_completed(result)
        assert_tool_called(ctx, "bash_executor")
        assert_llm_called(ctx)

    async def test_completes_without_llm(self) -> None:
        """GIVEN tools but no LLM WHEN run() THEN completed with fallback."""
        ctx = MockContext.create(
            tools={"bash_executor": {"stdout": "Cargo.toml"}},
            memory=True,
        )
        result = await SdkDemoAgent().run(_make_task(), ctx)

        assert_result_completed(result)
        text = result["output"][0]["text"]
        assert "LLM not available" in text

    async def test_completes_without_tools(self) -> None:
        """GIVEN LLM but no tools WHEN run() THEN completed (degraded)."""
        ctx = MockContext.create(
            llm_responses=[{"content": _LLM_ANALYSIS_JSON}],
            memory=True,
        )
        result = await SdkDemoAgent().run(_make_task(), ctx)

        assert_result_completed(result)


# ---------------------------------------------------------------------------
# Memory
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestMemory:
    """Verify memory interactions (recall + record + remember)."""

    async def test_records_and_recalls_memory(self) -> None:
        """GIVEN memory enabled WHEN run() THEN recall, record, remember."""
        ctx = MockContext.create(
            tools={"bash_executor": {"stdout": "src/"}},
            llm_responses=[{"content": _LLM_ANALYSIS_JSON}],
            memory=True,
        )
        await SdkDemoAgent().run(_make_task(), ctx)

        ops = [op["op"] for op in ctx.memory.operations]
        assert "recall" in ops
        assert "record" in ops
        assert "remember" in ops

    async def test_recall_returns_previous_value(self) -> None:
        """GIVEN a stored previous analysis WHEN run() THEN output shows it."""
        ctx = MockContext.create(
            tools={"bash_executor": {"stdout": "src/"}},
            llm_responses=[{"content": _LLM_ANALYSIS_JSON}],
            memory=True,
        )
        await ctx.memory.remember(
            "sdk_demo:last_analysis",
            "Previous run found 5 files.",
        )
        result = await SdkDemoAgent().run(_make_task(), ctx)

        text = result["output"][0]["text"]
        assert "Previous run found 5 files" in text


# ---------------------------------------------------------------------------
# Formatting
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestFormatting:
    """Verify the output contains Markdown table formatting."""

    async def test_output_contains_markdown_table(self) -> None:
        """GIVEN a full context WHEN run() THEN output has pipe characters."""
        ctx = MockContext.create(
            tools={"bash_executor": {"stdout": "file.rs"}},
            llm_responses=[{"content": _LLM_ANALYSIS_JSON}],
            memory=True,
        )
        result = await SdkDemoAgent().run(_make_task(), ctx)

        text = result["output"][0]["text"]
        assert "|" in text

    async def test_output_includes_analysis_summary(self) -> None:
        """GIVEN LLM response WHEN run() THEN summary appears in output."""
        ctx = MockContext.create(
            tools={"bash_executor": {"stdout": "crates/"}},
            llm_responses=[{"content": _LLM_ANALYSIS_JSON}],
            memory=True,
        )
        result = await SdkDemoAgent().run(_make_task(), ctx)

        text = result["output"][0]["text"]
        assert "well-structured Rust workspace" in text


# ---------------------------------------------------------------------------
# HITL
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestHitl:
    """Verify HITL suspension on destructive requests."""

    async def test_hitl_on_destructive_action(self) -> None:
        """GIVEN destructive input WHEN run() THEN input_required."""
        ctx = MockContext.create(
            tools={"bash_executor": {"stdout": ""}},
            llm_responses=[{"content": "{}"}],
            memory=True,
        )
        result = await SdkDemoAgent().run(
            _make_task("Delete all temporary files from the project"),
            ctx,
        )

        assert_result_input_required(result)
        prompt = result["input_required_data"]["prompt"]
        assert "destructive" in prompt.lower()

    async def test_resumes_after_approval(self) -> None:
        """GIVEN resumed task with approval WHEN run() THEN completed."""
        ctx = MockContext.create(
            tools={"bash_executor": {"stdout": "cleaned"}},
            llm_responses=[{"content": _LLM_ANALYSIS_JSON}],
            memory=True,
        )
        task = _make_task(
            "Delete all temporary files from the project",
            is_resumed=True,
            input_response={"approved": True, "reason": "confirmed"},
        )
        result = await SdkDemoAgent().run(task, ctx)

        assert_result_completed(result)


# ---------------------------------------------------------------------------
# Tool introspection
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestToolIntrospection:
    """Verify tool introspection via ctx.tools.describe."""

    async def test_output_includes_tool_description(self) -> None:
        """GIVEN tools available WHEN run() THEN describe() result in output."""
        ctx = MockContext.create(
            tools={"bash_executor": {"stdout": "ok"}},
            llm_responses=[{"content": _LLM_ANALYSIS_JSON}],
            memory=True,
        )
        result = await SdkDemoAgent().run(_make_task(), ctx)

        text = result["output"][0]["text"]
        assert "Mock tool bash_executor" in text

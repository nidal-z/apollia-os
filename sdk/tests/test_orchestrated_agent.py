"""Tests for OrchestratedAgent base class."""

from __future__ import annotations

from typing import Any

import pytest

from apollia.agents.orchestrated import OrchestratedAgent


class AnalyzerAgent(OrchestratedAgent):
    """Minimal concrete subclass for testing."""

    def manifest(self) -> dict[str, Any]:
        return {
            "name": "analyzer",
            "version": "0.1.0",
            "execution_mode": "orchestrated",
            "system_prompt": "Analyze the given data using available tools.",
            "tools_required": ["bash", "file_io"],
        }


@pytest.mark.asyncio
async def test_run_raises_runtime_error() -> None:
    """run() raises RuntimeError with informative message."""
    agent = AnalyzerAgent()
    with pytest.raises(RuntimeError, match="orchestrated"):
        await agent.run(None, None)


def test_on_plan_complete_default() -> None:
    """Default on_plan_complete auto-concatenates text outputs."""
    agent = AnalyzerAgent()
    results = {
        "step-1": {"status": "completed", "text": "Found 3 files"},
        "step-2": {"status": "completed", "text": "Analysis done"},
    }
    output = agent.on_plan_complete(results)
    assert "Found 3 files" in output["text"]
    assert "Analysis done" in output["text"]


def test_on_plan_complete_handles_string_results() -> None:
    """on_plan_complete handles plain string results alongside dicts."""
    agent = AnalyzerAgent()
    results: dict[str, Any] = {
        "step-1": {"text": "From dict"},
        "step-2": "Plain string result",
    }
    output = agent.on_plan_complete(results)
    assert "From dict" in output["text"]
    assert "Plain string result" in output["text"]


def test_on_plan_complete_empty_results() -> None:
    """on_plan_complete returns empty text for empty results."""
    agent = AnalyzerAgent()
    output = agent.on_plan_complete({})
    assert output == {"text": ""}


def test_format_step_results() -> None:
    """format_step_results produces readable output."""
    results: dict[str, Any] = {
        "step-1": {"status": "completed", "text": "OK"},
        "step-2": {"status": "failed", "text": "Error"},
    }
    formatted = OrchestratedAgent.format_step_results(results)
    assert "[step-1]" in formatted
    assert "(completed)" in formatted
    assert "[step-2]" in formatted
    assert "(failed)" in formatted


def test_format_step_results_plain_string() -> None:
    """format_step_results handles plain string values."""
    results: dict[str, Any] = {
        "step-1": "Just a string",
    }
    formatted = OrchestratedAgent.format_step_results(results)
    assert "[step-1] Just a string" in formatted


def test_manifest_execution_mode() -> None:
    """Manifest must include execution_mode='orchestrated'."""
    agent = AnalyzerAgent()
    m = agent.manifest()
    assert m["execution_mode"] == "orchestrated"

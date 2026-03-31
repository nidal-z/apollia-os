"""Tests for the excel-worker agent."""
from __future__ import annotations

import importlib.util
import sys
from pathlib import Path
from typing import Any

import pytest

# Make both the SDK and the agents directory importable.
_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(_REPO_ROOT / "sdk"))
sys.path.insert(0, str(_REPO_ROOT / "agents"))

from apollia.testing import MockContext  # noqa: E402 (after sys.path setup)

# Load excel-worker.py via its file path — the hyphen in the filename
# prevents direct import, so importlib.util is used instead.
_AGENT_PATH = _REPO_ROOT / "agents" / "excel-worker.py"
_spec = importlib.util.spec_from_file_location("excel_worker", _AGENT_PATH)
assert _spec is not None and _spec.loader is not None
excel_worker = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(excel_worker)  # type: ignore[union-attr]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _tool_call_json(tool: str, code: str = "...") -> str:
    """Return a JSON string representing a ReAct tool_call action."""
    import json

    return json.dumps({
        "thought": f"Je vais utiliser {tool}",
        "action": "tool_call",
        "tool": tool,
        "args": {"code": code, "timeout_secs": 30},
    })


def _final_answer_json(text: str) -> str:
    """Return a JSON string representing a ReAct final_answer action."""
    import json

    return json.dumps({"thought": "Terminé", "action": "final_answer", "text": text})


def _called_tools(ctx: Any) -> list[str]:
    """Return the ordered list of tool names called on *ctx*."""
    assert ctx.tools is not None
    return [name for name, _ in ctx.tools.calls]


# ---------------------------------------------------------------------------
# Manifest validation
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_manifest_is_valid() -> None:
    # GIVEN the module-level agent instance
    # WHEN manifest() is called
    m = excel_worker.agent.manifest()

    # THEN all required fields are present and correct
    assert m["name"] == "excel-worker"
    assert m["version"] == "0.1.0"
    assert "python_executor" in m["tools_required"]
    assert "file_read" in m["tools_required"]
    assert "file_write" in m["tools_required"]
    assert "openpyxl>=3.1.0" in m["packages"]
    assert m["supports_a2a"] is True
    assert m["execution_mode"] == "direct"
    assert m["max_concurrent_tasks"] == 1
    assert {s["id"] for s in m["skills"]} == {"read-excel", "edit-excel", "analyze-excel"}


# ---------------------------------------------------------------------------
# Guardrail verification
# ---------------------------------------------------------------------------


def test_guardrail_bash_forbidden() -> None:
    # GIVEN the SYSTEM_PROMPT constant
    # WHEN its text is inspected
    prompt = excel_worker.SYSTEM_PROMPT

    # THEN at least one of the expected guardrail phrases is present
    guardrail_phrases = [
        "JAMAIS bash",
        "JAMAIS bash_executor",
        "N'utilise JAMAIS bash",
        "Never use bash",
    ]
    assert any(phrase in prompt for phrase in guardrail_phrases), (
        "Guardrail phrase missing from SYSTEM_PROMPT. "
        f"Expected one of: {guardrail_phrases}"
    )


# ---------------------------------------------------------------------------
# ReAct loop — tool routing
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_read_uses_python_executor_not_bash() -> None:
    # GIVEN a context where python_executor returns sheet info
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "Feuilles: ['Ventes']\nLignes: 10",
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
            "file_write": {},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json("Fichier lu avec succès : 10 lignes dans la feuille Ventes.")},
        ],
    )

    # WHEN the agent runs the task
    agent = excel_worker.ExcelWorkerAgent()
    result = await agent.run({"input": {"text": "Lis ventes.xlsx"}}, ctx)

    # THEN the task completes and python_executor was used — never bash_executor
    assert result["status"] == "completed"
    called = _called_tools(ctx)
    assert "python_executor" in called
    assert "bash_executor" not in called


# ---------------------------------------------------------------------------
# Error path — corrupted file
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_corrupted_file_no_exception() -> None:
    # GIVEN python_executor returns a BadZipFile error
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "",
                "stderr": "zipfile.BadZipFile: File is not a zip file",
                "exit_code": 1,
            },
            "file_read": {"content": ""},
            "file_write": {},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json(
                "Le fichier est corrompu (BadZipFile). Impossible de le lire."
            )},
        ],
    )

    # WHEN the agent runs a read task on the corrupt file
    agent = excel_worker.ExcelWorkerAgent()
    result = await agent.run({"input": {"text": "Lis corrupt.xlsx"}}, ctx)

    # THEN the agent returns a valid status — no unhandled exception is raised
    assert result["status"] in ("failed", "completed")
    if result["status"] == "completed":
        output_text = " ".join(
            part.get("text", "") for part in result.get("output", [])
        )
        assert any(
            keyword in output_text
            for keyword in ("corrompu", "BadZipFile", "error", "Error")
        )

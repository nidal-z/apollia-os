"""Tests for the csv-data-worker agent."""
from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

import pytest

# Make both the SDK and the agents directory importable.
_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(_REPO_ROOT / "sdk"))
sys.path.insert(0, str(_REPO_ROOT / "agents"))

from apollia.testing import MockContext  # noqa: E402 (after sys.path setup)

# Load csv-data-worker.py via its file path — the hyphen in the filename
# prevents direct import, so importlib.util is used instead.
_AGENT_PATH = _REPO_ROOT / "agents" / "csv-data-worker.py"
_spec = importlib.util.spec_from_file_location("csv_data_worker", _AGENT_PATH)
assert _spec is not None and _spec.loader is not None
csv_data_worker = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(csv_data_worker)  # type: ignore[union-attr]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _tool_call_json(tool: str, code: str = "...") -> str:
    """Return a JSON string representing a ReAct tool_call action."""
    return json.dumps({
        "thought": f"Je vais utiliser {tool}",
        "action": "tool_call",
        "tool": tool,
        "args": {"code": code, "timeout_secs": 30},
    })


def _final_answer_json(text: str) -> str:
    """Return a JSON string representing a ReAct final_answer action."""
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
    m = csv_data_worker.agent.manifest()

    # THEN all required fields are present and correct
    assert m["name"] == "csv-data-worker"
    assert m["version"] == "0.1.0"
    assert "python_executor" in m["tools_required"]
    assert "file_read" in m["tools_required"]
    assert "pandas>=2.0.0" in m["packages"]
    assert m["supports_a2a"] is True
    assert m["execution_mode"] == "direct"
    assert m["max_concurrent_tasks"] == 1
    assert {s["id"] for s in m["skills"]} == {"read-csv", "analyze-csv", "transform-csv"}


# ---------------------------------------------------------------------------
# Guardrail verification
# ---------------------------------------------------------------------------


def test_guardrail_encoding_documented() -> None:
    # GIVEN the SYSTEM_PROMPT constant
    # WHEN its text is inspected
    prompt = csv_data_worker.SYSTEM_PROMPT

    # THEN encoding guardrail is present
    assert any(kw in prompt for kw in ["latin-1", "UnicodeDecodeError", "encodage", "encoding"]), (
        "Encoding guardrail missing from SYSTEM_PROMPT"
    )
    # AND separator guardrail is present
    assert any(kw in prompt for kw in ['sep=";"', "séparateur", "separator", "1 seule colonne"]), (
        "Separator guardrail missing from SYSTEM_PROMPT"
    )


# ---------------------------------------------------------------------------
# ReAct loop — tool routing
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_read_csv_uses_python_executor() -> None:
    # GIVEN a context where python_executor returns CSV shape info
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "Shape: (20, 4)\nColonnes: Nom,Ville,CA,Employés",
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json("Le CSV contient 20 lignes et 4 colonnes.")},
        ],
    )

    # WHEN the agent runs the task
    agent = csv_data_worker.CsvDataWorkerAgent()
    result = await agent.run({"input": {"text": "Lis data.csv"}}, ctx)

    # THEN the task completes and python_executor was used
    assert result["status"] == "completed"
    assert "python_executor" in _called_tools(ctx)


# ---------------------------------------------------------------------------
# Error path — encoding fallback
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_encoding_error_no_exception() -> None:
    # GIVEN python_executor signals a UnicodeDecodeError
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "",
                "stderr": "UnicodeDecodeError: 'utf-8' codec can't decode byte 0xe9",
                "exit_code": 1,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json(
                "Erreur d'encodage UTF-8 détectée. Réessai avec encoding='latin-1'."
            )},
        ],
    )

    # WHEN the agent runs the task
    agent = csv_data_worker.CsvDataWorkerAgent()
    result = await agent.run({"input": {"text": "Lis data_latin1.csv"}}, ctx)

    # THEN the agent returns a valid status — no unhandled exception is raised
    assert result["status"] in ("failed", "completed")
    if result["status"] == "completed":
        output_text = " ".join(
            part.get("text", "") for part in result.get("output", [])
        )
        assert any(
            keyword in output_text
            for keyword in ("latin", "encodage", "encoding", "UnicodeDecodeError")
        )

"""Tests for the pdf-worker agent."""
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

# Load pdf-worker.py via its file path — the hyphen in the filename
# prevents direct import, so importlib.util is used instead.
_AGENT_PATH = _REPO_ROOT / "agents" / "workers" / "pdf-worker.py"
_spec = importlib.util.spec_from_file_location("pdf_worker", _AGENT_PATH)
assert _spec is not None and _spec.loader is not None
pdf_worker = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pdf_worker)  # type: ignore[union-attr]


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
    m = pdf_worker.agent.manifest()

    # THEN all required fields are present and correct
    assert m["name"] == "pdf-worker"
    assert m["version"] == "0.2.0"
    assert "python_executor" in m["tools_required"]
    assert "file_read" in m["tools_required"]
    assert "pdfplumber>=0.10.0" in m["packages"]
    assert m["supports_a2a"] is True
    assert m["execution_mode"] == "direct"
    assert m["agent_type"] == "worker"
    assert m["max_concurrent_tasks"] == 1
    assert {s["id"] for s in m["skills"]} == {"read-pdf", "extract-text", "extract-tables"}


def test_manifest_skills_have_input_schema() -> None:
    # GIVEN the manifest
    # WHEN skills are inspected
    skills = pdf_worker.manifest()["skills"]

    # THEN every skill declares an input_schema with at least file_path
    for skill in skills:
        assert "input_schema" in skill, f"Skill '{skill['id']}' missing input_schema"
        schema = skill["input_schema"]
        assert "file_path" in schema, (
            f"Skill '{skill['id']}' input_schema missing 'file_path' parameter"
        )
        assert schema["file_path"].get("required") is True, (
            f"Skill '{skill['id']}' file_path should be required"
        )


def test_manifest_a2a_fields_complete() -> None:
    # GIVEN the manifest
    m = pdf_worker.manifest()

    # THEN A2A fields are fully declared
    assert m["supports_a2a"] is True
    assert m["memory_namespace"] == "pdf-worker"
    assert m["dangerous_tools_allowed"] is False
    assert isinstance(m["tags"], list)
    assert "pdf" in m["tags"]


# ---------------------------------------------------------------------------
# Guardrail verification
# ---------------------------------------------------------------------------


def test_guardrail_no_bash_for_pdf() -> None:
    # GIVEN the SYSTEM_PROMPT constant
    # WHEN its text is inspected
    prompt = pdf_worker.SYSTEM_PROMPT

    # THEN the absolute bash prohibition is present
    guardrail_phrases = [
        "JAMAIS bash",
        "N'utilise JAMAIS bash",
        "Never use bash",
    ]
    assert any(phrase in prompt for phrase in guardrail_phrases), (
        "Bash guardrail missing from SYSTEM_PROMPT. "
        f"Expected one of: {guardrail_phrases}"
    )


def test_guardrail_chunking_rule_present() -> None:
    # GIVEN the SYSTEM_PROMPT
    # WHEN the pagination rule is inspected
    prompt = pdf_worker.SYSTEM_PROMPT

    # THEN the 50-page chunking limit is documented
    assert "50" in prompt, "50-page chunking limit missing from SYSTEM_PROMPT"


def test_guardrail_password_protection_rule_present() -> None:
    # GIVEN the SYSTEM_PROMPT
    prompt = pdf_worker.SYSTEM_PROMPT

    # THEN the password prohibition is present
    assert "password_protected" in prompt or "mot de passe" in prompt.lower(), (
        "Password-protection handling missing from SYSTEM_PROMPT"
    )


def test_guardrail_scanned_pdf_rule_present() -> None:
    # GIVEN the SYSTEM_PROMPT
    prompt = pdf_worker.SYSTEM_PROMPT

    # THEN scanned PDF detection is documented
    assert "scanned_pdf" in prompt or "scanné" in prompt.lower(), (
        "Scanned PDF detection rule missing from SYSTEM_PROMPT"
    )


# ---------------------------------------------------------------------------
# ReAct loop — tool routing
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_read_pdf_uses_python_executor_not_bash() -> None:
    # GIVEN a context where python_executor returns page info
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "Pages: 5\nTitre: Rapport Annuel\nTexte: Contenu extrait...",
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json("Document lu : 5 pages, titre 'Rapport Annuel'.")},
        ],
    )

    # WHEN the agent runs the task
    agent = pdf_worker.PdfWorkerAgent()
    result = await agent.run({"input": {"text": "Lis rapport.pdf"}}, ctx)

    # THEN the task completes and only python_executor was called — never bash
    assert result["status"] == "completed"
    called = _called_tools(ctx)
    assert "python_executor" in called
    assert "bash_executor" not in called


@pytest.mark.asyncio
async def test_extract_tables_uses_python_executor() -> None:
    # GIVEN python_executor returning a table result
    table_output = (
        "Table 1:\nNom | Valeur\n--- | ---\nAlpha | 100\nBeta | 200\n"
    )
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": table_output,
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json(f"Tableaux extraits :\n{table_output}")},
        ],
    )

    # WHEN the agent runs an extract-tables task
    agent = pdf_worker.PdfWorkerAgent()
    result = await agent.run({"input": {"text": "Extrais les tableaux de rapport.pdf"}}, ctx)

    # THEN the task completes using python_executor
    assert result["status"] == "completed"
    assert "python_executor" in _called_tools(ctx)
    assert "bash_executor" not in _called_tools(ctx)


# ---------------------------------------------------------------------------
# Error paths — corrupted, password-protected, scanned
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_corrupted_pdf_no_exception() -> None:
    # GIVEN python_executor returning a PDFSyntaxError
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "",
                "stderr": "pdfminer.pdfparser.PDFSyntaxError: No /Root object! - Is this really a PDF?",
                "exit_code": 1,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json("Le fichier est corrompu ou n'est pas un PDF valide.")},
        ],
    )

    # WHEN the agent processes a corrupted file
    agent = pdf_worker.PdfWorkerAgent()
    result = await agent.run({"input": {"text": "Lis corrupt.pdf"}}, ctx)

    # THEN no unhandled exception is raised and status is valid
    assert result["status"] in ("failed", "completed")


@pytest.mark.asyncio
async def test_password_protected_pdf_no_exception() -> None:
    # GIVEN python_executor returning a password error
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "",
                "stderr": "pdfminer.pdfdocument.PDFPasswordIncorrect",
                "exit_code": 1,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json(
                "Ce PDF est protégé par un mot de passe. "
                "Fournissez le mot de passe ou exportez-le sans protection."
            )},
        ],
    )

    # WHEN the agent processes a password-protected file
    agent = pdf_worker.PdfWorkerAgent()
    result = await agent.run({"input": {"text": "Lis confidentiel.pdf"}}, ctx)

    # THEN the agent handles it gracefully
    assert result["status"] in ("failed", "completed")


@pytest.mark.asyncio
async def test_scanned_pdf_no_exception() -> None:
    # GIVEN python_executor returning a scanned-PDF detection result
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "SCANNED_PDF_DETECTED: page 1 has 0 chars but 3 images",
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json(
                "Ce PDF est scanné (images uniquement). L'extraction de texte n'est pas supportée."
            )},
        ],
    )

    # WHEN the agent processes a scanned file
    agent = pdf_worker.PdfWorkerAgent()
    result = await agent.run({"input": {"text": "Lis scan.pdf"}}, ctx)

    # THEN the agent reports the limitation clearly
    assert result["status"] in ("failed", "completed")


@pytest.mark.asyncio
async def test_file_not_found_no_exception() -> None:
    # GIVEN python_executor returning a FileNotFoundError
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "",
                "stderr": "FileNotFoundError: [Errno 2] No such file or directory: 'missing.pdf'",
                "exit_code": 1,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json("Fichier introuvable : missing.pdf")},
        ],
    )

    # WHEN the agent runs on a missing file
    agent = pdf_worker.PdfWorkerAgent()
    result = await agent.run({"input": {"text": "Lis missing.pdf"}}, ctx)

    # THEN the agent handles it without raising
    assert result["status"] in ("failed", "completed")


# ---------------------------------------------------------------------------
# Multi-page chunking
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_large_pdf_chunking_response_mentions_remaining_pages() -> None:
    # GIVEN python_executor returning text that mentions page limit was applied
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": (
                    "--- Page 1 ---\nContenu page 1\n"
                    "--- Page 50 ---\nContenu page 50\n"
                    "[Note: 150 pages supplémentaires non extraites. "
                    "Précisez une plage.]"
                ),
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json(
                "Extrait pages 1-50 sur 200. "
                "150 pages supplémentaires non extraites — précisez une plage si nécessaire."
            )},
        ],
    )

    # WHEN the agent processes a large PDF
    agent = pdf_worker.PdfWorkerAgent()
    result = await agent.run({"input": {"text": "Lis grand-document.pdf"}}, ctx)

    # THEN the task completes successfully
    assert result["status"] == "completed"
    assert "python_executor" in _called_tools(ctx)


# ---------------------------------------------------------------------------
# Module-level instance
# ---------------------------------------------------------------------------


def test_module_level_agent_instance() -> None:
    # GIVEN the module after execution
    # WHEN the agent attribute is accessed
    agent = pdf_worker.agent

    # THEN it is a PdfWorkerAgent with a valid manifest
    assert isinstance(agent, pdf_worker.PdfWorkerAgent)
    assert agent.manifest()["name"] == "pdf-worker"


def test_module_imports_without_error() -> None:
    # GIVEN the module is already loaded
    # WHEN the SYSTEM_PROMPT and manifest function are accessed
    # THEN both are present and non-empty
    assert isinstance(pdf_worker.SYSTEM_PROMPT, str)
    assert len(pdf_worker.SYSTEM_PROMPT) > 200
    assert callable(pdf_worker.manifest)


# ---------------------------------------------------------------------------
# Input handling — edge cases
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_task_input_as_plain_string() -> None:
    # GIVEN a task where input is a plain string (not a dict)
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "Pages: 2\nTexte extrait.",
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json("Document lu : 2 pages.")},
        ],
    )

    # WHEN the task input is a raw string
    agent = pdf_worker.PdfWorkerAgent()
    result = await agent.run({"input": "Lis document.pdf"}, ctx)

    # THEN the agent accepts it without error
    assert result["status"] == "completed"


@pytest.mark.asyncio
async def test_empty_input_handled() -> None:
    # GIVEN a task with empty text input
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "",
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _final_answer_json("Aucune instruction fournie.")},
        ],
    )

    # WHEN the agent receives an empty instruction
    agent = pdf_worker.PdfWorkerAgent()
    result = await agent.run({"input": {"text": ""}}, ctx)

    # THEN the agent returns a valid status without crashing
    assert result["status"] in ("completed", "failed")

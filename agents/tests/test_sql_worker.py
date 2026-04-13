"""Tests for the sql-worker agent."""
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

# Load sql-worker.py via its file path — the hyphen in the filename
# prevents direct import, so importlib.util is used instead.
_AGENT_PATH = _REPO_ROOT / "agents" / "workers" / "sql-worker.py"
_spec = importlib.util.spec_from_file_location("sql_worker", _AGENT_PATH)
assert _spec is not None and _spec.loader is not None
sql_worker = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(sql_worker)  # type: ignore[union-attr]


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
    m = sql_worker.agent.manifest()

    # THEN all required fields are present and correct
    assert m["name"] == "sql-worker"
    assert m["version"] == "0.1.0"
    assert "python_executor" in m["tools_required"]
    assert "file_read" in m["tools_required"]
    assert m["packages"] == []
    assert m["supports_a2a"] is True
    assert m["execution_mode"] == "direct"
    assert m["max_concurrent_tasks"] == 1
    assert {s["id"] for s in m["skills"]} == {"query-sql", "schema-inspect", "data-export"}
    assert m["dangerous_tools_allowed"] is False


def test_manifest_skills_have_input_schema() -> None:
    # GIVEN the manifest
    # WHEN skills are inspected
    skills = sql_worker.manifest()["skills"]

    # THEN every skill declares an input_schema
    for skill in skills:
        assert "input_schema" in skill, f"Skill '{skill['id']}' missing input_schema"


def test_manifest_a2a_fields_complete() -> None:
    # GIVEN the manifest
    m = sql_worker.manifest()

    # THEN A2A fields are fully declared
    assert m["supports_a2a"] is True
    assert m["memory_namespace"] == "sql-worker"
    assert m["dangerous_tools_allowed"] is False
    assert isinstance(m["tags"], list)
    assert "sql" in m["tags"]


# ---------------------------------------------------------------------------
# Guardrail verification (static analysis of SYSTEM_PROMPT)
# ---------------------------------------------------------------------------


def test_guardrail_no_mutation_by_default() -> None:
    # GIVEN the SYSTEM_PROMPT constant
    # WHEN its text is inspected
    prompt = sql_worker.SYSTEM_PROMPT

    # THEN the SELECT-only guardrail is present
    guardrail_phrases = [
        "SELECT UNIQUEMENT",
        "SELECT only",
        "JAMAIS de INSERT",
    ]
    assert any(phrase in prompt for phrase in guardrail_phrases), (
        f"SELECT-only guardrail missing from SYSTEM_PROMPT. "
        f"Expected one of: {guardrail_phrases}"
    )


def test_guardrail_parameterized_queries() -> None:
    # GIVEN the SYSTEM_PROMPT constant
    # WHEN its text is inspected
    prompt = sql_worker.SYSTEM_PROMPT

    # THEN the parameterisation guardrail is present
    assert "?" in prompt, "Bind parameter '?' missing from SYSTEM_PROMPT"
    param_phrases = ["paramètr", "parameter", "PARAMÉTRAGE"]
    assert any(phrase in prompt for phrase in param_phrases), (
        f"Parameterisation guardrail keyword missing from SYSTEM_PROMPT. "
        f"Expected one of: {param_phrases}"
    )


def test_guardrail_timeout_rule_present() -> None:
    # GIVEN the SYSTEM_PROMPT constant
    # WHEN its text is inspected
    prompt = sql_worker.SYSTEM_PROMPT

    # THEN the 30-second timeout rule is documented
    assert "30" in prompt, "30-second timeout guardrail missing from SYSTEM_PROMPT"


def test_guardrail_integrity_check_present() -> None:
    # GIVEN the SYSTEM_PROMPT constant
    prompt = sql_worker.SYSTEM_PROMPT

    # THEN the integrity check pattern is documented
    assert "integrity_check" in prompt or "intégrité" in prompt.lower(), (
        "PRAGMA integrity_check guardrail missing from SYSTEM_PROMPT"
    )


# ---------------------------------------------------------------------------
# ReAct loop — tool routing
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_query_uses_python_executor() -> None:
    # GIVEN a context where python_executor returns a query result
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "Résultat : 5 lignes\n| id | name |\n| 1 | Alice |",
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor", "import sqlite3...")},
            {"content": _final_answer_json("5 lignes trouvées dans la table clients.")},
        ],
    )

    # WHEN the agent runs the task
    agent = sql_worker.SqlWorkerAgent()
    result = await agent.run({"input": {"text": "Liste les clients"}}, ctx)

    # THEN the task completes and python_executor was called
    assert result["status"] == "completed"
    called = _called_tools(ctx)
    assert "python_executor" in called


@pytest.mark.asyncio
async def test_schema_inspect_uses_python_executor() -> None:
    # GIVEN a context where python_executor returns schema information
    schema_output = (
        "Tables: clients, commandes\n"
        "clients: id INTEGER, name TEXT, ca REAL\n"
        "commandes: id INTEGER, client_id INTEGER, montant REAL"
    )
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": schema_output,
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json(f"Schéma de la base :\n{schema_output}")},
        ],
    )

    # WHEN the agent inspects a schema
    agent = sql_worker.SqlWorkerAgent()
    result = await agent.run({"input": {"text": "Affiche le schéma de la base clients.db"}}, ctx)

    # THEN the task completes using python_executor
    assert result["status"] == "completed"
    assert "python_executor" in _called_tools(ctx)


# ---------------------------------------------------------------------------
# Error paths — domain errors
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_file_not_found_no_exception() -> None:
    # GIVEN python_executor returning a FileNotFoundError
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "",
                "stderr": "FileNotFoundError: [Errno 2] No such file or directory: 'missing.db'",
                "exit_code": 1,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json("Base de données introuvable : missing.db")},
        ],
    )

    # WHEN the agent processes a missing database
    agent = sql_worker.SqlWorkerAgent()
    result = await agent.run({"input": {"text": "Interroge missing.db"}}, ctx)

    # THEN the agent handles it gracefully without raising
    assert result["status"] in ("failed", "completed")


@pytest.mark.asyncio
async def test_corrupted_database_no_exception() -> None:
    # GIVEN python_executor returning a DatabaseError
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "",
                "stderr": "sqlite3.DatabaseError: database disk image is malformed",
                "exit_code": 1,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json("La base de données est corrompue.")},
        ],
    )

    # WHEN the agent processes a corrupted database
    agent = sql_worker.SqlWorkerAgent()
    result = await agent.run({"input": {"text": "Interroge corrupted.db"}}, ctx)

    # THEN no unhandled exception is raised
    assert result["status"] in ("failed", "completed")


@pytest.mark.asyncio
async def test_database_locked_no_exception() -> None:
    # GIVEN python_executor returning an OperationalError for a locked database
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "",
                "stderr": "sqlite3.OperationalError: database is locked",
                "exit_code": 1,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json(
                "La base est verrouillée par un autre processus."
            )},
        ],
    )

    # WHEN the agent encounters a locked database
    agent = sql_worker.SqlWorkerAgent()
    result = await agent.run({"input": {"text": "Interroge locked.db"}}, ctx)

    # THEN the agent handles it gracefully
    assert result["status"] in ("failed", "completed")


@pytest.mark.asyncio
async def test_sql_syntax_error_no_exception() -> None:
    # GIVEN python_executor returning an OperationalError for a syntax error
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "",
                "stderr": "sqlite3.OperationalError: near \"SELCT\": syntax error",
                "exit_code": 1,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json("Erreur SQL — vérifiez la syntaxe de la requête.")},
        ],
    )

    # WHEN the agent executes a malformed query
    agent = sql_worker.SqlWorkerAgent()
    result = await agent.run({"input": {"text": "SELCT * FROM clients"}}, ctx)

    # THEN the agent handles it gracefully
    assert result["status"] in ("failed", "completed")


# ---------------------------------------------------------------------------
# Module-level instance
# ---------------------------------------------------------------------------


def test_module_level_agent_instance() -> None:
    # GIVEN the module after execution
    # WHEN the agent attribute is accessed
    agent = sql_worker.agent

    # THEN it is a SqlWorkerAgent with a valid manifest
    assert isinstance(agent, sql_worker.SqlWorkerAgent)
    assert agent.manifest()["name"] == "sql-worker"


def test_module_imports_without_error() -> None:
    # GIVEN the module is already loaded
    # WHEN the SYSTEM_PROMPT and manifest function are accessed
    # THEN both are present and non-empty
    assert isinstance(sql_worker.SYSTEM_PROMPT, str)
    assert len(sql_worker.SYSTEM_PROMPT) > 200
    assert callable(sql_worker.manifest)


# ---------------------------------------------------------------------------
# Input handling — edge cases
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_task_input_as_plain_string() -> None:
    # GIVEN a task where input is a plain string (not a dict)
    ctx = MockContext.create(
        tools={
            "python_executor": {
                "stdout": "3 lignes trouvées.",
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("python_executor")},
            {"content": _final_answer_json("3 résultats trouvés.")},
        ],
    )

    # WHEN the task input is a raw string
    agent = sql_worker.SqlWorkerAgent()
    result = await agent.run({"input": "Compte les clients"}, ctx)

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
    agent = sql_worker.SqlWorkerAgent()
    result = await agent.run({"input": {"text": ""}}, ctx)

    # THEN the agent returns a valid status without crashing
    assert result["status"] in ("completed", "failed")

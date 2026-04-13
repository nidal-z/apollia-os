"""Tests for the code-worker agent."""
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

# Load code-worker.py via its file path — the hyphen in the filename
# prevents direct import, so importlib.util is used instead.
_AGENT_PATH = _REPO_ROOT / "agents" / "workers" / "code-worker.py"
_spec = importlib.util.spec_from_file_location("code_worker", _AGENT_PATH)
assert _spec is not None and _spec.loader is not None
code_worker = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(code_worker)  # type: ignore[union-attr]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _tool_call_json(tool: str, **args: Any) -> str:
    """Return a JSON string representing a ReAct tool_call action."""
    return json.dumps({
        "thought": f"Je vais utiliser {tool}",
        "action": "tool_call",
        "tool": tool,
        "args": args or {"cmd": "echo ok"},
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
    m = code_worker.agent.manifest()

    # THEN all required fields are present and correct
    assert m["name"] == "code-worker"
    assert m["version"] == "0.1.0"
    assert "bash_executor" in m["tools_required"]
    assert "file_read" in m["tools_required"]
    assert "file_write" in m["tools_required"]
    assert "file_edit" in m["tools_required"]
    assert m["packages"] == []
    assert m["supports_a2a"] is True
    assert m["execution_mode"] == "direct"
    assert m["max_concurrent_tasks"] == 1
    assert {s["id"] for s in m["skills"]} == {"generate-code", "refactor-code", "review-code"}


def test_manifest_skills_have_input_schema() -> None:
    # GIVEN the manifest
    # WHEN skills are inspected
    skills = code_worker.manifest()["skills"]

    # THEN every skill declares a non-empty input_schema
    for skill in skills:
        assert "input_schema" in skill, f"Skill '{skill['id']}' missing input_schema"
        assert len(skill["input_schema"]) >= 1, (
            f"Skill '{skill['id']}' input_schema is empty"
        )


def test_manifest_a2a_fields_complete() -> None:
    # GIVEN the manifest
    m = code_worker.manifest()

    # THEN A2A-related fields are fully declared
    assert m["supports_a2a"] is True
    assert m["memory_namespace"] == "code-worker"
    assert m["dangerous_tools_allowed"] is False
    assert isinstance(m["tags"], list)
    assert "code" in m["tags"]


# ---------------------------------------------------------------------------
# Guardrail verification (static SYSTEM_PROMPT inspection)
# ---------------------------------------------------------------------------


def test_guardrail_read_before_write() -> None:
    # GIVEN the SYSTEM_PROMPT constant
    # WHEN its text is inspected
    prompt = code_worker.SYSTEM_PROMPT

    # THEN the read-before-write enforcement phrase is present
    guardrail_phrases = [
        "lire le fichier",
        "file_read AVANT",
        "TOUJOURS lire",
        "read before",
    ]
    assert any(phrase in prompt for phrase in guardrail_phrases), (
        "Read-before-write guardrail missing from SYSTEM_PROMPT. "
        f"Expected one of: {guardrail_phrases}"
    )


def test_guardrail_syntax_verification() -> None:
    # GIVEN the SYSTEM_PROMPT constant
    # WHEN its text is inspected
    prompt = code_worker.SYSTEM_PROMPT

    # THEN both Python and Rust verification patterns are documented
    assert "ast.parse" in prompt, (
        "Python syntax verification (ast.parse) missing from SYSTEM_PROMPT"
    )
    assert "cargo check" in prompt, (
        "Rust compilation check (cargo check) missing from SYSTEM_PROMPT"
    )


def test_guardrail_review_format_present() -> None:
    # GIVEN the SYSTEM_PROMPT
    prompt = code_worker.SYSTEM_PROMPT

    # THEN the structured review format markers are documented
    assert "LGTM" in prompt, "LGTM marker missing from SYSTEM_PROMPT review format"
    assert "ISSUE" in prompt, "ISSUE marker missing from SYSTEM_PROMPT review format"


def test_guardrail_unsupported_language_rule_present() -> None:
    # GIVEN the SYSTEM_PROMPT
    prompt = code_worker.SYSTEM_PROMPT

    # THEN the V1 language scope is stated
    assert "unsupported_language" in prompt or "non supporté" in prompt.lower(), (
        "Unsupported language guardrail missing from SYSTEM_PROMPT"
    )


# ---------------------------------------------------------------------------
# Class constants
# ---------------------------------------------------------------------------


def test_class_constants() -> None:
    # GIVEN the CodeWorkerAgent class
    # WHEN its class-level constants are inspected
    # THEN they match the spec
    assert code_worker.CodeWorkerAgent.MAX_STEPS == 10
    assert code_worker.CodeWorkerAgent.TEMPERATURE == 0.1


def test_class_inherits_worker_agent() -> None:
    # GIVEN the CodeWorkerAgent class
    from apollia.agents import WorkerAgent

    # THEN it is a proper subclass of WorkerAgent
    assert issubclass(code_worker.CodeWorkerAgent, WorkerAgent)


# ---------------------------------------------------------------------------
# ReAct loop — tool routing
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_generate_code_uses_bash_executor_for_python() -> None:
    # GIVEN a context where file tools and bash_executor are available
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "file_write": {"success": True},
            "file_edit": {"success": True},
            "bash_executor": {"stdout": "", "stderr": "", "exit_code": 0},
        },
        llm_responses=[
            {"content": _tool_call_json(
                "bash_executor",
                cmd="python -c 'import ast; ast.parse(open(\"output.py\").read())'",
            )},
            {"content": _final_answer_json(
                "Fichier output.py créé : 20 lignes, 2 fonctions."
            )},
        ],
    )

    # WHEN the agent runs a generate-code task
    agent = code_worker.CodeWorkerAgent()
    result = await agent.run(
        {"input": {"text": "Génère un script Python qui lit un CSV et affiche les totaux"}},
        ctx,
    )

    # THEN the task completes
    assert result["status"] == "completed"
    assert "bash_executor" in _called_tools(ctx)


@pytest.mark.asyncio
async def test_refactor_code_reads_file_before_writing() -> None:
    # GIVEN a context simulating a file that needs refactoring
    original_content = "def foo():\n    x = 1\n    y = 2\n    return x + y\n"
    ctx = MockContext.create(
        tools={
            "file_read": {"content": original_content},
            "file_write": {"success": True},
            "file_edit": {"success": True},
            "bash_executor": {"stdout": "", "stderr": "", "exit_code": 0},
        },
        llm_responses=[
            {"content": _tool_call_json("file_read", path="script.py")},
            {"content": _tool_call_json("file_write", path="script.py", content="...")},
            {"content": _final_answer_json("Refactoring terminé : 4 → 3 lignes.")},
        ],
    )

    # WHEN the agent runs a refactor-code task
    agent = code_worker.CodeWorkerAgent()
    result = await agent.run(
        {"input": {"text": "Refactore script.py pour utiliser une seule expression de retour"}},
        ctx,
    )

    # THEN the task completes and file_read was called before file_write
    assert result["status"] == "completed"
    tool_order = _called_tools(ctx)
    assert "file_read" in tool_order
    assert "file_write" in tool_order
    assert tool_order.index("file_read") < tool_order.index("file_write")


@pytest.mark.asyncio
async def test_review_code_uses_file_read() -> None:
    # GIVEN a context with a readable Python file
    ctx = MockContext.create(
        tools={
            "file_read": {
                "content": (
                    "def divide(a, b):\n"
                    "    return a / b\n"
                )
            },
            "file_write": {"success": True},
            "file_edit": {"success": True},
            "bash_executor": {"stdout": "", "stderr": "", "exit_code": 0},
        },
        llm_responses=[
            {"content": _tool_call_json("file_read", path="utils.py")},
            {"content": _final_answer_json(
                "🔴 ISSUE ligne 2 : division par zéro non protégée — ajouter `if b == 0: raise ValueError`."
            )},
        ],
    )

    # WHEN the agent performs a code review
    agent = code_worker.CodeWorkerAgent()
    result = await agent.run(
        {"input": {"text": "Fais une revue de code de utils.py"}},
        ctx,
    )

    # THEN the task completes using file_read
    assert result["status"] == "completed"
    assert "file_read" in _called_tools(ctx)


# ---------------------------------------------------------------------------
# Error paths
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_syntax_error_python_no_exception() -> None:
    # GIVEN bash_executor returning a Python syntax error
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "file_write": {"success": True},
            "file_edit": {"success": True},
            "bash_executor": {
                "stdout": "",
                "stderr": "SyntaxError: invalid syntax (output.py, line 3)",
                "exit_code": 1,
            },
        },
        llm_responses=[
            {"content": _tool_call_json(
                "bash_executor",
                cmd="python -c 'import ast; ast.parse(open(\"output.py\").read())'",
            )},
            {"content": _final_answer_json(
                "Erreur de syntaxe Python à la ligne 3. Correction appliquée."
            )},
        ],
    )

    # WHEN the agent processes a file with a syntax error
    agent = code_worker.CodeWorkerAgent()
    result = await agent.run(
        {"input": {"text": "Génère output.py"}},
        ctx,
    )

    # THEN no unhandled exception is raised and status is valid
    assert result["status"] in ("failed", "completed")


@pytest.mark.asyncio
async def test_rust_compilation_error_no_exception() -> None:
    # GIVEN bash_executor returning a Rust compilation error
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "file_write": {"success": True},
            "file_edit": {"success": True},
            "bash_executor": {
                "stdout": "",
                "stderr": "error[E0308]: mismatched types --> src/main.rs:5:9",
                "exit_code": 1,
            },
        },
        llm_responses=[
            {"content": _tool_call_json(
                "bash_executor",
                cmd="cargo check --message-format=short 2>&1 | head -20",
            )},
            {"content": _final_answer_json(
                "Erreur de compilation Rust ligne 5 : type mismatch. Correction en cours."
            )},
        ],
    )

    # WHEN the agent processes a Rust file with compilation errors
    agent = code_worker.CodeWorkerAgent()
    result = await agent.run(
        {"input": {"text": "Génère src/main.rs"}},
        ctx,
    )

    # THEN the agent handles it gracefully
    assert result["status"] in ("failed", "completed")


@pytest.mark.asyncio
async def test_file_not_found_no_exception() -> None:
    # GIVEN file_read returning a FileNotFoundError
    ctx = MockContext.create(
        tools={
            "file_read": {"error": "FileNotFoundError: missing.py not found"},
            "file_write": {"success": True},
            "file_edit": {"success": True},
            "bash_executor": {"stdout": "", "stderr": "", "exit_code": 0},
        },
        llm_responses=[
            {"content": _tool_call_json("file_read", path="missing.py")},
            {"content": _final_answer_json("Fichier introuvable : missing.py")},
        ],
    )

    # WHEN the agent tries to refactor a missing file
    agent = code_worker.CodeWorkerAgent()
    result = await agent.run(
        {"input": {"text": "Refactore missing.py"}},
        ctx,
    )

    # THEN the agent handles it without raising
    assert result["status"] in ("failed", "completed")


# ---------------------------------------------------------------------------
# Input handling — edge cases
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_task_input_as_plain_string() -> None:
    # GIVEN a task where input is a plain string (not a dict)
    ctx = MockContext.create(
        tools={
            "file_read": {"content": "x = 1\n"},
            "file_write": {"success": True},
            "file_edit": {"success": True},
            "bash_executor": {"stdout": "", "stderr": "", "exit_code": 0},
        },
        llm_responses=[
            {"content": _final_answer_json("Revue terminée : 🟢 LGTM.")},
        ],
    )

    # WHEN the task input is a raw string
    agent = code_worker.CodeWorkerAgent()
    result = await agent.run({"input": "Revue de code de script.py"}, ctx)

    # THEN the agent accepts it without error
    assert result["status"] in ("completed", "failed")


@pytest.mark.asyncio
async def test_empty_input_handled() -> None:
    # GIVEN a task with empty text input
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "file_write": {"success": True},
            "file_edit": {"success": True},
            "bash_executor": {"stdout": "", "stderr": "", "exit_code": 0},
        },
        llm_responses=[
            {"content": _final_answer_json("Aucune instruction fournie.")},
        ],
    )

    # WHEN the agent receives an empty instruction
    agent = code_worker.CodeWorkerAgent()
    result = await agent.run({"input": {"text": ""}}, ctx)

    # THEN the agent returns a valid status without crashing
    assert result["status"] in ("completed", "failed")


# ---------------------------------------------------------------------------
# Module-level instance
# ---------------------------------------------------------------------------


def test_module_level_agent_instance() -> None:
    # GIVEN the module after execution
    # WHEN the agent attribute is accessed
    agent = code_worker.agent

    # THEN it is a CodeWorkerAgent with a valid manifest
    assert isinstance(agent, code_worker.CodeWorkerAgent)
    assert agent.manifest()["name"] == "code-worker"


def test_module_imports_without_error() -> None:
    # GIVEN the module is already loaded
    # WHEN the SYSTEM_PROMPT and manifest function are accessed
    # THEN both are present and non-empty
    assert isinstance(code_worker.SYSTEM_PROMPT, str)
    assert len(code_worker.SYSTEM_PROMPT) > 200
    assert callable(code_worker.manifest)

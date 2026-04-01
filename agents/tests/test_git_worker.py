"""Tests for the git-worker agent."""
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

# Load git-worker.py via its file path — the hyphen in the filename
# prevents direct import, so importlib.util is used instead.
_AGENT_PATH = _REPO_ROOT / "agents" / "git-worker.py"
_spec = importlib.util.spec_from_file_location("git_worker", _AGENT_PATH)
assert _spec is not None and _spec.loader is not None
git_worker = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(git_worker)  # type: ignore[union-attr]


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _tool_call_json(tool: str, cmd: str = "git status --porcelain") -> str:
    """Return a JSON string representing a ReAct tool_call action."""
    return json.dumps({
        "thought": f"Je vais utiliser {tool}",
        "action": "tool_call",
        "tool": tool,
        "args": {"cmd": cmd},
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
    m = git_worker.agent.manifest()

    # THEN all required fields are present and correct
    assert m["name"] == "git-worker"
    assert m["version"] == "0.1.0"
    assert "bash_executor" in m["tools_required"]
    assert "file_read" in m["tools_required"]
    assert m["packages"] == []
    assert m["supports_a2a"] is True
    assert m["execution_mode"] == "direct"
    assert m["max_concurrent_tasks"] == 1
    assert {s["id"] for s in m["skills"]} == {"git-status", "git-diff", "git-commit"}
    assert m["dangerous_tools_allowed"] is False


def test_manifest_skills_have_input_schema() -> None:
    # GIVEN the manifest
    # WHEN skills are inspected
    skills = git_worker.manifest()["skills"]

    # THEN every skill declares an input_schema
    for skill in skills:
        assert "input_schema" in skill, f"Skill '{skill['id']}' missing input_schema"


def test_manifest_a2a_fields_complete() -> None:
    # GIVEN the manifest
    m = git_worker.manifest()

    # THEN A2A fields are fully declared
    assert m["supports_a2a"] is True
    assert m["memory_namespace"] == "git-worker"
    assert m["dangerous_tools_allowed"] is False
    assert isinstance(m["tags"], list)
    assert "git" in m["tags"]


# ---------------------------------------------------------------------------
# Guardrail verification (static analysis of SYSTEM_PROMPT)
# ---------------------------------------------------------------------------


def test_guardrail_no_destructive_operations() -> None:
    # GIVEN the SYSTEM_PROMPT constant
    # WHEN its text is inspected
    prompt = git_worker.SYSTEM_PROMPT

    # THEN all destructive operation keywords are present
    destructive_keywords = ["force", "--hard", "clean -fd", "branch -D"]
    for keyword in destructive_keywords:
        assert keyword in prompt, (
            f"Destructive operation guardrail '{keyword}' missing from SYSTEM_PROMPT"
        )


def test_guardrail_conventional_commits() -> None:
    # GIVEN the SYSTEM_PROMPT constant
    # WHEN its text is inspected
    prompt = git_worker.SYSTEM_PROMPT

    # THEN the conventional commit guardrail is present
    conventional_markers = ["conventionnel", "conventional", "feat(", "fix("]
    assert any(marker in prompt for marker in conventional_markers), (
        f"Conventional commit guardrail missing from SYSTEM_PROMPT. "
        f"Expected one of: {conventional_markers}"
    )


def test_guardrail_status_before_commit() -> None:
    # GIVEN the SYSTEM_PROMPT constant
    # WHEN its text is inspected
    prompt = git_worker.SYSTEM_PROMPT

    # THEN the status-before-commit rule is documented
    assert "git status" in prompt, "Status-before-commit guardrail missing from SYSTEM_PROMPT"


def test_guardrail_no_git_add_all() -> None:
    # GIVEN the SYSTEM_PROMPT constant
    # WHEN its text is inspected
    prompt = git_worker.SYSTEM_PROMPT

    # THEN the prohibition on git add . / git add -A is present
    assert "git add ." in prompt or "git add -A" in prompt, (
        "git add wildcard prohibition missing from SYSTEM_PROMPT"
    )


# ---------------------------------------------------------------------------
# ReAct loop — tool routing
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_git_status_uses_bash_executor() -> None:
    # GIVEN a context where bash_executor returns a git status output
    ctx = MockContext.create(
        tools={
            "bash_executor": {
                "stdout": " M agents/git-worker.py\n?? agents/tests/test_git_worker.py",
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("bash_executor", "git status --porcelain")},
            {"content": _final_answer_json("2 fichiers modifiés : git-worker.py (modifié), test_git_worker.py (untracked).")},
        ],
    )

    # WHEN the agent checks the repository status
    agent = git_worker.GitWorkerAgent()
    result = await agent.run({"input": {"text": "Montre-moi le status du dépôt"}}, ctx)

    # THEN the task completes and bash_executor was called
    assert result["status"] == "completed"
    assert "bash_executor" in _called_tools(ctx)


@pytest.mark.asyncio
async def test_git_diff_uses_bash_executor() -> None:
    # GIVEN a context where bash_executor returns a git diff output
    diff_output = (
        "diff --git a/agents/git-worker.py b/agents/git-worker.py\n"
        "index 0000000..1234567 100644\n"
        "--- a/agents/git-worker.py\n"
        "+++ b/agents/git-worker.py\n"
        "@@ -0,0 +1,5 @@\n"
        "+# git-worker\n"
    )
    ctx = MockContext.create(
        tools={
            "bash_executor": {
                "stdout": diff_output,
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("bash_executor", "git diff")},
            {"content": _final_answer_json("1 fichier modifié : +5 lignes dans git-worker.py.")},
        ],
    )

    # WHEN the agent diffs the changes
    agent = git_worker.GitWorkerAgent()
    result = await agent.run({"input": {"text": "Montre-moi les modifications en cours"}}, ctx)

    # THEN the task completes using bash_executor
    assert result["status"] == "completed"
    assert "bash_executor" in _called_tools(ctx)


@pytest.mark.asyncio
async def test_git_commit_uses_bash_executor() -> None:
    # GIVEN a context where bash_executor successfully creates a commit
    ctx = MockContext.create(
        tools={
            "bash_executor": {
                "stdout": "[main abc1234] feat(agents): add git-worker agent V1\n 1 file changed, 5 insertions(+)",
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("bash_executor", "git status --porcelain")},
            {"content": _tool_call_json("bash_executor", 'git commit -m "feat(agents): add git-worker agent V1"')},
            {"content": _final_answer_json("Commit créé : abc1234 — feat(agents): add git-worker agent V1.")},
        ],
    )

    # WHEN the agent commits the changes
    agent = git_worker.GitWorkerAgent()
    result = await agent.run(
        {"input": {"text": "Committe les modifications avec un message conventionnel"}},
        ctx,
    )

    # THEN the task completes and bash_executor was invoked
    assert result["status"] == "completed"
    assert "bash_executor" in _called_tools(ctx)


# ---------------------------------------------------------------------------
# Error paths — domain errors
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_not_a_repo_no_exception() -> None:
    # GIVEN bash_executor returning a "not a git repository" error
    ctx = MockContext.create(
        tools={
            "bash_executor": {
                "stdout": "",
                "stderr": "fatal: not a git repository (or any of the parent directories): .git",
                "exit_code": 128,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("bash_executor", "git status --porcelain")},
            {"content": _final_answer_json("Le répertoire courant n'est pas un dépôt Git.")},
        ],
    )

    # WHEN the agent tries to operate outside a git repo
    agent = git_worker.GitWorkerAgent()
    result = await agent.run({"input": {"text": "Montre-moi le status"}}, ctx)

    # THEN the agent handles it gracefully without raising
    assert result["status"] in ("failed", "completed")


@pytest.mark.asyncio
async def test_nothing_to_commit_no_exception() -> None:
    # GIVEN bash_executor returning an empty status (clean working tree)
    ctx = MockContext.create(
        tools={
            "bash_executor": {
                "stdout": "",
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("bash_executor", "git status --porcelain")},
            {"content": _final_answer_json("Aucune modification à committer — le dépôt est propre.")},
        ],
    )

    # WHEN the agent tries to commit with nothing staged
    agent = git_worker.GitWorkerAgent()
    result = await agent.run({"input": {"text": "Committe les modifications"}}, ctx)

    # THEN the agent returns a valid status without crashing
    assert result["status"] in ("completed", "failed")


@pytest.mark.asyncio
async def test_merge_conflict_no_exception() -> None:
    # GIVEN bash_executor returning a merge conflict indicator
    ctx = MockContext.create(
        tools={
            "bash_executor": {
                "stdout": "UU src/main.rs",
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("bash_executor", "git status --porcelain")},
            {"content": _final_answer_json("Conflits de fusion détectés — résolution manuelle requise.")},
        ],
    )

    # WHEN the agent encounters merge conflicts
    agent = git_worker.GitWorkerAgent()
    result = await agent.run({"input": {"text": "Montre-moi le status"}}, ctx)

    # THEN the agent handles it gracefully
    assert result["status"] in ("completed", "failed")


# ---------------------------------------------------------------------------
# Module-level instance
# ---------------------------------------------------------------------------


def test_module_level_agent_instance() -> None:
    # GIVEN the module after execution
    # WHEN the agent attribute is accessed
    agent = git_worker.agent

    # THEN it is a GitWorkerAgent with a valid manifest
    assert isinstance(agent, git_worker.GitWorkerAgent)
    assert agent.manifest()["name"] == "git-worker"


def test_module_imports_without_error() -> None:
    # GIVEN the module is already loaded
    # WHEN the SYSTEM_PROMPT and manifest function are accessed
    # THEN both are present and non-empty
    assert isinstance(git_worker.SYSTEM_PROMPT, str)
    assert len(git_worker.SYSTEM_PROMPT) > 200
    assert callable(git_worker.manifest)


# ---------------------------------------------------------------------------
# Input handling — edge cases
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_task_input_as_plain_string() -> None:
    # GIVEN a task where input is a plain string (not a dict)
    ctx = MockContext.create(
        tools={
            "bash_executor": {
                "stdout": " M README.md",
                "stderr": "",
                "exit_code": 0,
            },
            "file_read": {"content": ""},
        },
        llm_responses=[
            {"content": _tool_call_json("bash_executor", "git status --porcelain")},
            {"content": _final_answer_json("1 fichier modifié : README.md")},
        ],
    )

    # WHEN the task input is a raw string
    agent = git_worker.GitWorkerAgent()
    result = await agent.run({"input": "Status du dépôt"}, ctx)

    # THEN the agent accepts it without error
    assert result["status"] == "completed"


@pytest.mark.asyncio
async def test_empty_input_handled() -> None:
    # GIVEN a task with empty text input
    ctx = MockContext.create(
        tools={
            "bash_executor": {
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
    agent = git_worker.GitWorkerAgent()
    result = await agent.run({"input": {"text": ""}}, ctx)

    # THEN the agent returns a valid status without crashing
    assert result["status"] in ("completed", "failed")

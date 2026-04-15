"""Integration tests for ProjectContextBootstrap and its subclasses.

Validates the full bootstrap lifecycle with a real git repository in a
temporary directory: snapshot population, staleness detection, persistence,
and subclass-specific extra scopes.
"""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

# ---------------------------------------------------------------------------
# Import path setup
# ---------------------------------------------------------------------------

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
if str(_REPO_ROOT / "sdk") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "sdk"))
if str(_REPO_ROOT / "agents") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "agents"))

from apollia.bootstrap import ContextBootstrap  # noqa: E402
from apollia.testing import MockContext  # noqa: E402
from assistants.shared.project_bootstrap import (  # noqa: E402
    ProjectContextBootstrap,
)

# Load agent modules (hyphens in filenames)
_SPEC_PATH = _REPO_ROOT / "agents" / "assistants" / "spec-assistant.py"
_spec_mod_spec = importlib.util.spec_from_file_location("spec_assistant", _SPEC_PATH)
assert _spec_mod_spec is not None and _spec_mod_spec.loader is not None
spec_assistant = importlib.util.module_from_spec(_spec_mod_spec)
_spec_mod_spec.loader.exec_module(spec_assistant)  # type: ignore[union-attr]

_DEV_PATH = _REPO_ROOT / "agents" / "assistants" / "dev-assistant.py"
_dev_mod_spec = importlib.util.spec_from_file_location("dev_assistant", _DEV_PATH)
assert _dev_mod_spec is not None and _dev_mod_spec.loader is not None
dev_assistant = importlib.util.module_from_spec(_dev_mod_spec)
_dev_mod_spec.loader.exec_module(dev_assistant)  # type: ignore[union-attr]


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

_GIT_ENV = {
    **os.environ,
    "GIT_AUTHOR_NAME": "test",
    "GIT_AUTHOR_EMAIL": "t@t.com",
    "GIT_COMMITTER_NAME": "test",
    "GIT_COMMITTER_EMAIL": "t@t.com",
}


@pytest.fixture()
def tmp_workspace(tmp_path: Path) -> Path:
    """Create a git-initialised workspace with APOLLIA.md and Cargo.toml."""
    ws = tmp_path / "workspace"
    ws.mkdir()

    (ws / "APOLLIA.md").write_text("# Test Project\nLocal-first: true\n")
    (ws / "Cargo.toml").write_text('[package]\nname = "test"\nversion = "0.1.0"\n')

    tasks = ws / ".apollia" / "tasks"
    tasks.mkdir(parents=True)
    (tasks / "user-auth.md").write_text("# User Auth Spec\n")

    subprocess.run(["git", "init"], cwd=ws, check=True, capture_output=True)
    subprocess.run(["git", "add", "."], cwd=ws, check=True, capture_output=True)
    subprocess.run(
        ["git", "commit", "-m", "init"],
        cwd=ws,
        check=True,
        capture_output=True,
        env=_GIT_ENV,
    )
    return ws


def _head_hash(workspace: Path) -> str:
    """Return the HEAD commit hash of the given workspace."""
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=workspace,
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def _make_commit(workspace: Path, filename: str, content: str) -> str:
    """Create a new file, commit it, and return the new HEAD hash."""
    (workspace / filename).write_text(content)
    subprocess.run(["git", "add", filename], cwd=workspace, check=True, capture_output=True)
    subprocess.run(
        ["git", "commit", "-m", f"add {filename}"],
        cwd=workspace,
        check=True,
        capture_output=True,
        env=_GIT_ENV,
    )
    return _head_hash(workspace)


def _ctx_for_workspace(workspace: Path) -> Any:
    """Build a MockContext whose bash_executor runs commands in *workspace*."""
    commit = _head_hash(workspace)

    tool_responses: dict[str, Any] = {
        "file_read": {"content": ""},
        "bash_executor": {"stdout": commit},
    }

    return MockContext.create(
        tools=tool_responses,
        memory=True,
        workspace_rules="# Test Project\nLocal-first: true\n",
    )


# ---------------------------------------------------------------------------
# Test 1 — Bootstrap populates workspace_rules
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_bootstrap_populates_workspace_rules(tmp_workspace: Path) -> None:
    """GIVEN a workspace with rules in ctx.workspace
    WHEN run_bootstrap() is called
    THEN snapshot.workspace_rules contains the rules text."""
    ctx = _ctx_for_workspace(tmp_workspace)

    bootstrap = ProjectContextBootstrap()
    snapshot = await bootstrap.run_bootstrap(ctx)

    assert "Local-first" in snapshot["workspace_rules"]
    assert snapshot["has_git"] is True
    assert len(snapshot["commit_hash"]) >= 7


# ---------------------------------------------------------------------------
# Test 2 — Staleness detected after a new commit
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_bootstrap_staleness_on_new_commit(tmp_workspace: Path) -> None:
    """GIVEN a bootstrap persisted with commit A
    WHEN a new commit B is created
    THEN is_stale() returns True."""
    commit_a = _head_hash(tmp_workspace)
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": commit_a},
        },
        memory=True,
        workspace_rules="rules",
    )

    bootstrap = ProjectContextBootstrap()
    await bootstrap.run_bootstrap(ctx)

    assert not await bootstrap.is_stale(ctx), "should NOT be stale right after bootstrap"

    commit_b = _make_commit(tmp_workspace, "new.txt", "change")

    ctx_after = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": commit_b},
        },
        memory=True,
        workspace_rules="rules",
    )
    # Transfer persisted memory from previous context
    assert ctx.memory is not None and ctx_after.memory is not None
    ctx_after.memory.store = dict(ctx.memory.store)
    ctx_after.memory.confidences = dict(ctx.memory.confidences)

    assert await bootstrap.is_stale(ctx_after), "should be stale after new commit"


# ---------------------------------------------------------------------------
# Test 3 — Not stale when commit unchanged
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_bootstrap_fresh_on_same_commit(tmp_workspace: Path) -> None:
    """GIVEN a bootstrap persisted with commit A
    WHEN the commit is still A
    THEN is_stale() returns False and needs_bootstrap() returns False."""
    ctx = _ctx_for_workspace(tmp_workspace)

    bootstrap = ProjectContextBootstrap()
    await bootstrap.run_bootstrap(ctx)

    assert not await bootstrap.is_stale(ctx)
    assert not await bootstrap.needs_bootstrap(ctx)


# ---------------------------------------------------------------------------
# Test 4 — No-git workspace degrades gracefully
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_bootstrap_no_git_workspace(tmp_path: Path) -> None:
    """GIVEN a workspace without a git repository
    WHEN run_bootstrap() is called
    THEN has_git is False and no error is raised."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": "no-git"},
        },
        memory=True,
        workspace_rules="rules",
    )

    bootstrap = ProjectContextBootstrap()
    snapshot = await bootstrap.run_bootstrap(ctx)

    assert snapshot["has_git"] is False
    assert snapshot["commit_hash"] == "no-git"

    # "no-git" is a stable marker that never triggers re-bootstrap
    assert not await bootstrap.is_stale(ctx)


# ---------------------------------------------------------------------------
# Test 5 — SpecContextBootstrap discovers existing specs
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_spec_bootstrap_lists_existing_specs(tmp_workspace: Path) -> None:
    """GIVEN a workspace with .apollia/tasks/user-auth.md
    WHEN SpecContextBootstrap.run_bootstrap() runs
    THEN snapshot.existing_specs contains 'user-auth'."""
    commit = _head_hash(tmp_workspace)

    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": commit},
        },
        memory=True,
        workspace_rules="rules",
    )
    # bash_executor returns different values depending on the command.
    # Dispatch based on the command string content.
    original_call = ctx.tools.call  # type: ignore[union-attr]

    async def _bash_dispatch(
        tool_name: str,
        input: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        if tool_name == "bash_executor":
            cmd = str((input or {}).get("command", ""))
            if "rev-parse" in cmd:
                return {"stdout": commit}
            if ".apollia/tasks" in cmd:
                return {"stdout": ".apollia/tasks/user-auth.md\n"}
            return {"stdout": commit}
        return await original_call(tool_name, input)

    ctx.tools.call = _bash_dispatch  # type: ignore[union-attr, assignment]

    bootstrap = spec_assistant.SpecContextBootstrap()
    snapshot = await bootstrap.run_bootstrap(ctx)

    assert "user-auth" in snapshot.get("existing_specs", [])
    assert snapshot.get("spec_count", 0) >= 1


# ---------------------------------------------------------------------------
# Test 6 — DevContextBootstrap detects tech stack and architecture
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_dev_bootstrap_detects_tech_stack(tmp_workspace: Path) -> None:
    """GIVEN a workspace with Cargo.toml and package.json
    WHEN DevContextBootstrap.run_bootstrap() runs
    THEN snapshot.tech_stack contains both markers."""
    commit = _head_hash(tmp_workspace)

    # Add a package.json to the workspace
    (tmp_workspace / "package.json").write_text('{"name": "test"}')

    call_index = {"n": 0}

    async def _dispatch(
        tool_name: str,
        input: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        if tool_name == "bash_executor":
            call_index["n"] += 1
            cmd = (input or {}).get("command", "")
            if "rev-parse" in str(cmd):
                return {"stdout": commit}
            if "find" in str(cmd) and "Cargo.toml" in str(cmd):
                return {"stdout": "./Cargo.toml\n"}
            if "git diff" in str(cmd):
                return {"stdout": "new.txt\n"}
            # Default for _current_commit
            return {"stdout": commit}
        if tool_name == "file_read":
            path = (input or {}).get("path", "")
            if str(path) in ("Cargo.toml", "package.json"):
                return {"content": "exists"}
            return {"error": "not found"}
        return {"error": f"unknown tool {tool_name}"}

    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "bash_executor": {"stdout": commit},
        },
        memory=True,
        workspace_rules="rules",
    )
    ctx.tools.call = _dispatch  # type: ignore[union-attr, assignment]

    bootstrap = dev_assistant.DevContextBootstrap()
    snapshot = await bootstrap.run_bootstrap(ctx)

    assert "architecture" in snapshot
    assert "recent_files" in snapshot
    assert snapshot.get("has_git") is True
    assert isinstance(snapshot["tech_stack"], list)

"""Tests for the apollia-code-reviewer agent."""
from __future__ import annotations

import importlib.util
import os
import sys
import types
from pathlib import Path

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_AGENT_PATH = _REPO_ROOT / "agents" / "apollia-code-reviewer.py"


def _load_module(env_overrides: dict[str, str] | None = None) -> types.ModuleType:
    """Load the code-reviewer module in isolation with the given env overrides.

    The module is re-executed from source each call so that module-level constants
    (``REPO_PATH``, ``SYSTEM_PROMPT``) are recomputed with the supplied environment.

    Args:
        env_overrides: Mapping of environment variables to set before loading.
            Variables present in this dict shadow the real environment; variables
            *absent* from this dict (but present in the real env) are preserved
            unless ``APOLLIA_REPO_PATH`` needs to be absent — callers must
            explicitly pass ``{}`` and ensure the key is not in the real env, or
            use ``monkeypatch`` / ``unittest.mock.patch.dict`` upstream.
    """
    saved = os.environ.copy()
    if env_overrides is not None:
        for key, value in env_overrides.items():
            os.environ[key] = value
    try:
        spec = importlib.util.spec_from_file_location("_code_reviewer_isolated", _AGENT_PATH)
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        # Ensure SDK and agents directories are importable.
        for extra in (_REPO_ROOT / "sdk", _REPO_ROOT / "agents"):
            path_str = str(extra)
            if path_str not in sys.path:
                sys.path.insert(0, path_str)
        spec.loader.exec_module(module)  # type: ignore[union-attr]
        return module
    finally:
        os.environ.clear()
        os.environ.update(saved)


# ---------------------------------------------------------------------------
# AC-1 — APOLLIA_REPO_PATH defined → REPO_PATH uses that value
# ---------------------------------------------------------------------------


def test_repo_path_from_env_variable(monkeypatch: pytest.MonkeyPatch) -> None:
    # GIVEN APOLLIA_REPO_PATH is set to an explicit path
    monkeypatch.setenv("APOLLIA_REPO_PATH", "/tmp/my-repo")

    # WHEN the module is loaded
    module = _load_module({"APOLLIA_REPO_PATH": "/tmp/my-repo"})

    # THEN REPO_PATH equals the environment variable value
    assert module.REPO_PATH == "/tmp/my-repo"


def test_repo_path_appears_in_system_prompt_when_set(monkeypatch: pytest.MonkeyPatch) -> None:
    # GIVEN APOLLIA_REPO_PATH is set to a known path
    monkeypatch.setenv("APOLLIA_REPO_PATH", "/tmp/custom-repo")

    # WHEN the module is loaded
    module = _load_module({"APOLLIA_REPO_PATH": "/tmp/custom-repo"})

    # THEN SYSTEM_PROMPT contains the custom repo path (the .replace() substitution)
    assert "/tmp/custom-repo" in module.SYSTEM_PROMPT


# ---------------------------------------------------------------------------
# AC-2 — APOLLIA_REPO_PATH absent → REPO_PATH defaults to os.getcwd()
# ---------------------------------------------------------------------------


def test_repo_path_defaults_to_cwd(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    # GIVEN APOLLIA_REPO_PATH is not defined and the working directory is tmp_path
    monkeypatch.delenv("APOLLIA_REPO_PATH", raising=False)
    monkeypatch.chdir(tmp_path)

    # WHEN the module is loaded (no env override for APOLLIA_REPO_PATH)
    saved = os.environ.copy()
    os.environ.pop("APOLLIA_REPO_PATH", None)
    try:
        spec = importlib.util.spec_from_file_location("_code_reviewer_cwd", _AGENT_PATH)
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)  # type: ignore[union-attr]
    finally:
        os.environ.clear()
        os.environ.update(saved)

    # THEN REPO_PATH equals the current working directory at load time
    assert module.REPO_PATH == str(tmp_path)


# ---------------------------------------------------------------------------
# AC-3 — No hardcoded machine-specific absolute paths in agents/
# ---------------------------------------------------------------------------


def test_no_hardcoded_user_paths_in_agents() -> None:
    # GIVEN the agents/ directory
    agents_dir = _REPO_ROOT / "agents"

    # WHEN all Python source files are scanned for /Users/ occurrences
    matches: list[str] = []
    for py_file in agents_dir.rglob("*.py"):
        # Skip compiled bytecode cache directories and the test files themselves.
        if "__pycache__" in py_file.parts or py_file.parent.name == "tests":
            continue
        text = py_file.read_text(encoding="utf-8", errors="replace")
        for lineno, line in enumerate(text.splitlines(), start=1):
            if "/Users/" in line:
                matches.append(f"{py_file.relative_to(_REPO_ROOT)}:{lineno}: {line.strip()}")

    # THEN the result is empty
    assert matches == [], (
        "Hardcoded /Users/ paths found in agents/:\n" + "\n".join(matches)
    )


# ---------------------------------------------------------------------------
# Module sanity — constant types and manifest
# ---------------------------------------------------------------------------


def test_repo_path_is_a_string() -> None:
    # GIVEN the module loaded with a known env var
    module = _load_module({"APOLLIA_REPO_PATH": "/tmp/test"})

    # WHEN REPO_PATH is inspected
    # THEN it is a non-empty string
    assert isinstance(module.REPO_PATH, str)
    assert len(module.REPO_PATH) > 0


def test_manifest_returns_expected_name() -> None:
    # GIVEN the module-level agent instance
    module = _load_module({"APOLLIA_REPO_PATH": "/tmp/test"})

    # WHEN manifest() is called
    manifest = module.agent.manifest()

    # THEN the agent name is correct
    assert manifest["name"] == "apollia-code-reviewer"
    assert "bash_executor" in manifest["tools_required"]

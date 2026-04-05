"""Tests for the apollia-review agent."""
from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_AGENT_PATH = _REPO_ROOT / "agents" / "apollia-review.py"

# Make SDK importable.
for extra in (_REPO_ROOT / "sdk", _REPO_ROOT / "agents"):
    path_str = str(extra)
    if path_str not in sys.path:
        sys.path.insert(0, path_str)


def _load_module() -> Any:
    spec = importlib.util.spec_from_file_location("apollia_review", _AGENT_PATH)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    # Register before exec so @dataclass can resolve forward references.
    sys.modules["apollia_review"] = module
    try:
        spec.loader.exec_module(module)  # type: ignore[union-attr]
    except Exception:
        del sys.modules["apollia_review"]
        raise
    return module


review_mod = _load_module()


# ─── Helpers ──────────────────────────────────────────────────────────────────

class _MockTools:
    def __init__(self, results: dict[str, Any] | None = None) -> None:
        self._results: dict[str, Any] = results or {}
        self.calls: list[tuple[str, dict[str, Any]]] = []

    async def call(self, tool_name: str, args: dict[str, Any] | None = None) -> Any:
        self.calls.append((tool_name, args or {}))
        if tool_name in self._results:
            return self._results[tool_name]
        raise RuntimeError(f"MockTools: '{tool_name}' not configured")

    def called_with(self, tool_name: str) -> bool:
        return any(name == tool_name for name, _ in self.calls)

    def last_args(self, tool_name: str) -> dict[str, Any] | None:
        for name, args in reversed(self.calls):
            if name == tool_name:
                return args
        return None


class _MockLlm:
    def __init__(self, responses: list[str] | None = None) -> None:
        self._responses = list(responses or [])
        self._index = 0
        self.call_count = 0

    async def complete(self, messages: list[Any], **kwargs: Any) -> MagicMock:
        self.call_count += 1
        mock = MagicMock()
        if self._index < len(self._responses):
            mock.content = self._responses[self._index]
            self._index += 1
        else:
            mock.content = '{"score": 100, "issues": [], "summary": "ok"}'
        return mock


class _MockCtx:
    def __init__(
        self,
        tools: _MockTools | None = None,
        llm: _MockLlm | None = None,
        task: dict[str, Any] | None = None,
    ) -> None:
        self.tools = tools if tools is not None else _MockTools()
        self.llm = llm if llm is not None else _MockLlm()
        self.task = task or {}


def _bash_result(output: str) -> dict[str, Any]:
    return {"output": output}


# ─── Manifest validation ──────────────────────────────────────────────────────

def test_manifest_returns_required_name() -> None:
    # GIVEN the apollia-review module
    # WHEN manifest() is called
    m = review_mod.manifest()
    # THEN name is correct
    assert m["name"] == "apollia-review"


def test_manifest_returns_version() -> None:
    # GIVEN the apollia-review module
    # WHEN manifest() is called
    m = review_mod.manifest()
    # THEN version is present
    assert m["version"] == "0.1.0"


def test_manifest_contains_required_inputs() -> None:
    # GIVEN the apollia-review module
    # WHEN manifest() is called
    m = review_mod.manifest()
    inputs = m["inputs"]
    # THEN all four documented inputs are present
    for key in ("task_id", "pr_number", "diff_file", "review_type"):
        assert key in inputs, f"input '{key}' missing from manifest"


def test_manifest_requires_bash_executor() -> None:
    # GIVEN the manifest
    m = review_mod.manifest()
    # THEN bash_executor is in tools_required
    assert "bash_executor" in m["tools_required"]


# ─── Empty diff → score 100 ───────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_empty_diff_returns_score_100() -> None:
    # GIVEN a context where git diff HEAD~1 returns empty string
    tools = _MockTools({
        "bash_executor": _bash_result(""),
    })
    ctx = _MockCtx(tools=tools)

    # WHEN run() is called with no inputs
    result = await review_mod.run(ctx)

    # THEN score is 100 and summary signals no changes
    assert result["score"] == 100
    assert "Aucun changement" in result["summary"]


@pytest.mark.asyncio
async def test_whitespace_only_diff_returns_score_100() -> None:
    # GIVEN git diff returns only whitespace
    tools = _MockTools({"bash_executor": _bash_result("   \n  ")})
    ctx = _MockCtx(tools=tools)

    result = await review_mod.run(ctx)

    assert result["score"] == 100


# ─── PR review calls `gh pr diff N` ──────────────────────────────────────────

@pytest.mark.asyncio
async def test_pr_number_triggers_gh_pr_diff() -> None:
    # GIVEN a context with pr_number = 42 and a valid LLM response
    diff_output = "diff --git a/src/lib.rs b/src/lib.rs\n+fn foo() {}"
    tools = _MockTools({"bash_executor": _bash_result(diff_output)})
    llm = _MockLlm(['{"score": 90, "issues": [], "summary": "looks good"}'])
    ctx = _MockCtx(
        tools=tools,
        llm=llm,
        task={"input": {"pr_number": 42}},
    )

    # WHEN run() is called
    await review_mod.run(ctx)

    # THEN the first bash_executor call used `gh pr diff 42`
    assert len(tools.calls) >= 1
    first_cmd = tools.calls[0][1].get("command", "")
    assert "gh pr diff 42" in first_cmd


# ─── Diff file source ─────────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_diff_file_reads_file_via_cat() -> None:
    # GIVEN a context with diff_file set
    tools = _MockTools({"bash_executor": _bash_result("diff --git a/f b/f\n+x = 1")})
    llm = _MockLlm(['{"score": 80, "issues": [], "summary": "minor"}'])
    ctx = _MockCtx(
        tools=tools,
        llm=llm,
        task={"input": {"diff_file": "/tmp/my.patch"}},
    )

    await review_mod.run(ctx)

    assert len(tools.calls) >= 1
    first_cmd = tools.calls[0][1].get("command", "")
    assert "cat /tmp/my.patch" in first_cmd


# ─── Fallback to git diff HEAD~1 ─────────────────────────────────────────────

@pytest.mark.asyncio
async def test_no_inputs_falls_back_to_git_diff() -> None:
    # GIVEN no pr_number, diff_file, or task_id
    diff = "diff --git a/x.py b/x.py\n+y = 2"
    tools = _MockTools({"bash_executor": _bash_result(diff)})
    llm = _MockLlm(['{"score": 95, "issues": [], "summary": "clean"}'])
    ctx = _MockCtx(tools=tools, llm=llm)

    await review_mod.run(ctx)

    # THEN the first bash call used git diff HEAD~1
    first_call = tools.calls[0]
    assert "git diff HEAD~1" in first_call[1].get("command", "")


# ─── LLM parsing failure → degraded report ───────────────────────────────────

@pytest.mark.asyncio
async def test_non_json_llm_response_returns_score_0() -> None:
    # GIVEN the LLM returns non-JSON text
    diff = "diff --git a/a.py b/a.py\n+x = 1"
    tools = _MockTools({"bash_executor": _bash_result(diff)})
    llm = _MockLlm(["This is not JSON at all."])
    ctx = _MockCtx(tools=tools, llm=llm)

    result = await review_mod.run(ctx)

    assert result["score"] == 0
    assert "Erreur parsing" in result["summary"]


# ─── LLM unavailable → degraded report ───────────────────────────────────────

@pytest.mark.asyncio
async def test_no_llm_returns_degraded_report() -> None:
    # GIVEN ctx.llm is None
    diff = "diff --git a/b.py b/b.py\n+z = 3"
    _tools = _MockTools({"bash_executor": _bash_result(diff)})

    class _NoLlmCtx:
        pass

    no_llm_ctx = _NoLlmCtx()
    no_llm_ctx.tools = _tools  # type: ignore[attr-defined]
    no_llm_ctx.llm = None  # type: ignore[attr-defined]
    no_llm_ctx.task = {}  # type: ignore[attr-defined]

    result = await review_mod.run(no_llm_ctx)

    assert result["score"] == 0
    assert "Erreur parsing" in result["summary"]


# ─── Score clamping ───────────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_score_is_clamped_to_0_100() -> None:
    # GIVEN the LLM returns a score outside [0, 100]
    diff = "diff --git a/c.py b/c.py\n+a = 0"
    tools = _MockTools({"bash_executor": _bash_result(diff)})
    llm = _MockLlm(['{"score": 999, "issues": [], "summary": "impossible score"}'])
    ctx = _MockCtx(tools=tools, llm=llm)

    result = await review_mod.run(ctx)

    assert result["score"] <= 100


# ─── Valid full report structure ──────────────────────────────────────────────

@pytest.mark.asyncio
async def test_full_report_structure_is_correct() -> None:
    # GIVEN the LLM returns a report with one issue
    diff = "diff --git a/d.rs b/d.rs\n+fn foo() { panic!() }"
    tools = _MockTools({"bash_executor": _bash_result(diff)})
    llm_json = json.dumps({
        "score": 60,
        "issues": [
            {
                "severity": "error",
                "category": "correctness",
                "message": "use of panic! in production code",
                "location": "d.rs:1",
            }
        ],
        "summary": "Critical: panic! found.",
    })
    llm = _MockLlm([llm_json])
    ctx = _MockCtx(tools=tools, llm=llm)

    result = await review_mod.run(ctx)

    assert result["score"] == 60
    assert len(result["issues"]) == 1
    issue = result["issues"][0]
    assert issue["severity"] == "error"
    assert issue["category"] == "correctness"
    assert issue["location"] == "d.rs:1"
    assert "summary" in result

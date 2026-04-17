"""Cross-cutting smoke tests for the 4 ReactAgent-based assistants (v2).

Three tiers of guarantee:

1. **Manifest** — required fields present and coherent for every assistant.
2. **Behaviour** — each assistant routes tool calls correctly when the LLM
   decides to call them (file_write for spec, a2a:* for document, …).
3. **Crash resistance** — no assistant raises an unhandled exception on a
   trivial message.

These tests don't measure LLM response quality. They assert the *surface*:
tool invocations, status, manifest integrity.
"""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

import pytest

# ---------------------------------------------------------------------------
# Import path setup — must precede any apollia import
# ---------------------------------------------------------------------------

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
for _extra in (str(_REPO_ROOT / "sdk"), str(_REPO_ROOT / "agents")):
    if _extra not in sys.path:
        sys.path.insert(0, _extra)

from apollia.testing import MockContext  # noqa: E402


# ---------------------------------------------------------------------------
# Module loading (hyphens in filenames prohibit direct import)
# ---------------------------------------------------------------------------


def _load_agent_module(name: str) -> Any:
    """Load an assistant module by its hyphenated filename."""
    path = _REPO_ROOT / "agents" / "assistants" / f"{name}.py"
    module_name = name.replace("-", "_")
    spec = importlib.util.spec_from_file_location(module_name, path)
    assert spec is not None and spec.loader is not None, (
        f"Could not locate assistant module: {path}"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    return module


_spec_assistant = _load_agent_module("spec-assistant")
_dev_assistant = _load_agent_module("dev-assistant")
_review_assistant = _load_agent_module("review-assistant")
_document_assistant = _load_agent_module("document-assistant")


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

ASSISTANTS = [
    ("spec-assistant", _spec_assistant),
    ("dev-assistant", _dev_assistant),
    ("review-assistant", _review_assistant),
    ("document-assistant", _document_assistant),
]

REQUIRED_MANIFEST_FIELDS = [
    "name",
    "version",
    "description",
    "tools_required",
    "supports_a2a",
    "memory_namespace",
    "execution_mode",
    "agent_type",
]


# ---------------------------------------------------------------------------
# Helpers — ReAct JSON builders
# ---------------------------------------------------------------------------


def _tool_call(tool: str, **args: Any) -> str:
    """ReAct JSON for a tool_call action."""
    return json.dumps({
        "thought": f"Calling {tool}",
        "action": "tool_call",
        "tool": tool,
        "args": args,
    })


def _final_answer(text: str) -> str:
    """ReAct JSON for a final_answer action."""
    return json.dumps({
        "thought": "Done",
        "action": "final_answer",
        "text": text,
    })


def _make_task(text: str) -> dict[str, Any]:
    """Build a minimal task dict in the Apollia runtime format."""
    return {"input": {"parts": [{"text": text}]}}


# ---------------------------------------------------------------------------
# Tier 1 — Manifests
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("agent_name,module", ASSISTANTS)
def test_manifest_has_required_fields(agent_name: str, module: Any) -> None:
    """Every assistant manifest declares every required field with a value."""
    m = module.manifest()

    for field in REQUIRED_MANIFEST_FIELDS:
        assert field in m, f"{agent_name}: manifest field '{field}' missing"
        assert m[field] is not None, f"{agent_name}: manifest '{field}' is None"
        if isinstance(m[field], str):
            assert len(m[field]) > 0, (
                f"{agent_name}: manifest '{field}' is an empty string"
            )

    assert m["name"] == agent_name, (
        f"{agent_name}: manifest name is '{m['name']}'"
    )
    assert m["supports_a2a"] is True, (
        f"{agent_name}: supports_a2a must be True"
    )


@pytest.mark.parametrize("agent_name,module", ASSISTANTS)
def test_manifest_all_v2(agent_name: str, module: Any) -> None:
    """All assistants have been bumped to v2 after the ReactAgent migration."""
    m = module.manifest()
    assert m["version"].startswith("2."), (
        f"{agent_name}: expected v2.x, got {m['version']}"
    )


@pytest.mark.parametrize("agent_name,module", ASSISTANTS)
def test_is_react_agent(agent_name: str, module: Any) -> None:
    """Every assistant inherits from BaseReActAgent."""
    from apollia.agents import BaseReActAgent

    assert isinstance(module.agent, BaseReActAgent), (
        f"{agent_name}: module.agent is not a BaseReActAgent"
    )


# ---------------------------------------------------------------------------
# Tier 2 — Behaviour : spec-assistant writes TaskSpec via file_write tool
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_spec_assistant_writes_taskspec_via_file_write() -> None:
    """When the LLM emits tool_call(file_write, ...), the spec file is saved."""
    spec_body = (
        "# TaskSpec — Export CSV button\n\n"
        "## Objective\nAdd a CSV export button.\n\n"
        "## Acceptance criteria\n- [ ] Button visible in production\n"
    )
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "file_write": {"success": True},
            "bash_executor": {"stdout": "abc123", "stderr": "", "exit_code": 0},
            "ask_user": {"answers": []},
        },
        llm_responses=[
            {"content": _tool_call(
                "file_write",
                path=".apollia/tasks/export-csv.md",
                content=spec_body,
            )},
            {"content": _final_answer(
                "Saved to .apollia/tasks/export-csv.md. Please review."
            )},
        ],
        memory=True,
    )

    result = await _spec_assistant.SpecAssistant().run(
        _make_task("create a spec for a CSV export button"),
        ctx,
    )

    assert result["status"] == "completed"
    assert ctx.tools is not None
    file_writes = [
        args for name, args in ctx.tools.calls
        if name == "file_write"
    ]
    assert any(
        ".apollia/tasks/" in str(args.get("path", ""))
        for args in file_writes
    ), f"no file_write targeting .apollia/tasks/; got: {file_writes}"


# ---------------------------------------------------------------------------
# Tier 2 — Behaviour : document-assistant routes to a2a:* tools
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_document_assistant_routes_xlsx_via_a2a_tool() -> None:
    """When the LLM picks a2a:analyze-excel, the tool is invoked."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "ask_user": {"answers": []},
            "a2a:analyze-excel": {"output": "Total: 42,580 €"},
        },
        llm_responses=[
            {"content": _tool_call(
                "a2a:analyze-excel",
                task="Sum column A",
                file="ventes.xlsx",
            )},
            {"content": _final_answer("The total is 42,580 €.")},
        ],
        memory=True,
    )

    result = await _document_assistant.DocumentAssistant().run(
        _make_task("What is the total of column A in ventes.xlsx?"),
        ctx,
    )

    assert result["status"] == "completed"
    assert ctx.tools is not None
    assert any(
        name == "a2a:analyze-excel" for name, _ in ctx.tools.calls
    ), f"a2a:analyze-excel not called; got: {[n for n, _ in ctx.tools.calls]}"


@pytest.mark.asyncio
async def test_document_assistant_routes_pdf_via_a2a_tool() -> None:
    """When the LLM picks a2a:extract-text for a PDF, the tool is invoked."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "ask_user": {"answers": []},
            "a2a:extract-text": {"output": "Key points: +12% growth."},
        },
        llm_responses=[
            {"content": _tool_call(
                "a2a:extract-text",
                task="Summarise the main points",
                file="report.pdf",
            )},
            {"content": _final_answer("Key point: +12% growth.")},
        ],
        memory=True,
    )

    result = await _document_assistant.DocumentAssistant().run(
        _make_task("Summarise report.pdf"),
        ctx,
    )

    assert result["status"] == "completed"
    assert ctx.tools is not None
    assert any(
        name == "a2a:extract-text" for name, _ in ctx.tools.calls
    ), f"a2a:extract-text not called; got: {[n for n, _ in ctx.tools.calls]}"


# ---------------------------------------------------------------------------
# Tier 3 — Crash resistance
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
@pytest.mark.parametrize("agent_name,module", ASSISTANTS)
async def test_agent_does_not_crash_on_hello(agent_name: str, module: Any) -> None:
    """No assistant raises on a trivial 'hello' message.

    The LLM is mocked to return final_answer immediately so no tools are
    invoked — purely a plumbing / exception-path check.
    """
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "file_write": {"success": True},
            "bash_executor": {"stdout": "abc123", "stderr": "", "exit_code": 0},
            "ask_user": {"answers": []},
        },
        llm_responses=[
            {"content": _final_answer("Hello, how can I help you?")},
        ],
        memory=True,
    )

    try:
        result = await module.agent.run(_make_task("hello"), ctx)
    except Exception as exc:  # pragma: no cover — fail the test explicitly
        pytest.fail(f"{agent_name} raised on 'hello': {exc!r}")

    assert result is not None, f"{agent_name}: run() returned None"
    assert result.get("status") in ("completed", "input_required", "failed"), (
        f"{agent_name}: unexpected status '{result.get('status')}'"
    )


# ---------------------------------------------------------------------------
# Tier 4 — Snapshot persistence across sessions (spec-assistant)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_spec_assistant_bootstrap_persists_snapshot() -> None:
    """After a first session, the bootstrap snapshot is written to memory."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "file_write": {"success": True},
            "bash_executor": {"stdout": "abc123", "stderr": "", "exit_code": 0},
            "ask_user": {"answers": []},
        },
        llm_responses=[
            {"content": _final_answer("Anything")},
        ],
        memory=True,
        workspace_rules="# Rules\n- no anyhow",
    )

    agent = _spec_assistant.SpecAssistant()
    await agent.run(_make_task("create a spec"), ctx)

    assert ctx.memory is not None
    status = await ctx.memory.recall("bootstrap.status")
    assert status == "complete", (
        "bootstrap status should be 'complete' after first session"
    )


@pytest.mark.asyncio
async def test_dev_assistant_snapshot_contains_architecture() -> None:
    """After running dev-assistant, the snapshot has an architecture key."""
    ctx = MockContext.create(
        tools={
            "file_read": {"content": ""},
            "file_write": {"success": True},
            "bash_executor": {
                "stdout": "./crates/core/Cargo.toml\n./crates/runtime/Cargo.toml\n",
                "stderr": "",
                "exit_code": 0,
            },
            "ask_user": {"answers": []},
        },
        llm_responses=[
            {"content": _final_answer("Here is an overview.")},
        ],
        memory=True,
        workspace_rules="anyhow INTERDIT",
    )

    agent = _dev_assistant.DevAssistant()
    await agent.run(_make_task("Explain the architecture"), ctx)

    assert ctx.memory is not None
    raw_snapshot = await ctx.memory.recall("bootstrap.snapshot")
    assert raw_snapshot is not None, "bootstrap snapshot should be persisted"

    snapshot = json.loads(raw_snapshot)
    assert "architecture" in snapshot, "snapshot must contain 'architecture'"
    assert isinstance(snapshot["architecture"], list)

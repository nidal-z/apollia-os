"""Tests unitaires pour agents/apollia-reviewer.py.

Aucun runtime Rust requis. Tout ctx est mocké.
Lance avec : .venv-agents/bin/pytest tests/python/ -v
"""
import importlib.util
import os
import pytest
from unittest.mock import AsyncMock, MagicMock

# ---------------------------------------------------------------------------
# Chargement du module agent (nom de fichier avec tiret - pas importable via import)
# ---------------------------------------------------------------------------

def _load_agent_module():
    spec = importlib.util.spec_from_file_location(
        "apollia_reviewer",
        os.path.join(os.path.dirname(__file__), "../../agents/apollia-reviewer.py"),
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

_mod = _load_agent_module()
ApollaReviewer = _mod.ApollaReviewer


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def make_task(text: str, task_id: str = "t-test-001") -> dict:
    return {
        "task_id": task_id,
        "input": {"parts": [{"type": "text", "text": text}]},
        "history": [],
    }


def make_ctx(*, has_llm: bool = False, has_memory: bool = True, git_stdout: str = "mock git output"):
    ctx = MagicMock()

    # Tous les appels d'outils retournent une sortie git simulée par défaut
    ctx.tools.call = AsyncMock(return_value={
        "stdout": git_stdout,
        "stderr": "",
        "exit_code": 0,
    })

    if has_memory:
        ctx.memory = MagicMock()
        ctx.memory.record = AsyncMock(return_value=None)
        ctx.memory.search = AsyncMock(return_value=[])
    else:
        ctx.memory = None

    if has_llm:
        llm_response = MagicMock()
        llm_response.content = "**Aucun problème majeur.** Le diff est propre."
        ctx.llm = MagicMock()
        ctx.llm.chat = AsyncMock(return_value=llm_response)
    else:
        ctx.llm = None

    ctx.log = MagicMock()
    ctx.step_budget = MagicMock()
    ctx.step_budget.steps_remaining = 50

    return ctx


# ---------------------------------------------------------------------------
# Tests manifest
# ---------------------------------------------------------------------------

class TestManifest:
    def test_name(self):
        m = ApollaReviewer().manifest()
        assert m["name"] == "apollia-reviewer"

    def test_version_semver(self):
        m = ApollaReviewer().manifest()
        parts = m["version"].split(".")
        assert len(parts) == 3
        assert all(p.isdigit() for p in parts)

    def test_tools_required_present(self):
        m = ApollaReviewer().manifest()
        assert "bash_executor" in m["tools_required"]
        assert "file_io" in m["tools_required"]

    def test_memory_namespace_declared(self):
        m = ApollaReviewer().manifest()
        assert "memory_namespace" in m
        assert m["memory_namespace"]  # non-vide

    def test_execution_mode_direct(self):
        m = ApollaReviewer().manifest()
        assert m.get("execution_mode") == "direct"

    def test_required_keys_present(self):
        m = ApollaReviewer().manifest()
        for key in ("name", "version", "description", "tools_required"):
            assert key in m, f"Clé obligatoire absente : {key}"


# ---------------------------------------------------------------------------
# Tests run() - validation d'entrée
# ---------------------------------------------------------------------------

class TestInputValidation:
    @pytest.mark.asyncio
    async def test_empty_string_returns_failed(self):
        result = await ApollaReviewer().run(make_task(""), make_ctx())
        assert result["status"] == "failed"
        assert result["error"]["code"] == "MISSING_INPUT"

    @pytest.mark.asyncio
    async def test_whitespace_only_returns_failed(self):
        result = await ApollaReviewer().run(make_task("   \n"), make_ctx())
        assert result["status"] == "failed"

    @pytest.mark.asyncio
    async def test_task_id_propagated(self):
        result = await ApollaReviewer().run(make_task("/repo", task_id="t-42"), make_ctx())
        assert result["task_id"] == "t-42"

    @pytest.mark.asyncio
    async def test_output_is_list(self):
        result = await ApollaReviewer().run(make_task("/repo"), make_ctx())
        assert isinstance(result["output"], list)
        assert len(result["output"]) > 0

    @pytest.mark.asyncio
    async def test_output_first_part_is_text(self):
        result = await ApollaReviewer().run(make_task("/repo"), make_ctx())
        assert result["output"][0]["type"] == "text"
        assert isinstance(result["output"][0]["text"], str)


# ---------------------------------------------------------------------------
# Tests run() - flux nominal (Palier 0)
# ---------------------------------------------------------------------------

class TestRunTier0:
    @pytest.mark.asyncio
    async def test_returns_completed(self):
        result = await ApollaReviewer().run(make_task("/repo"), make_ctx())
        assert result["status"] == "completed"

    @pytest.mark.asyncio
    async def test_calls_bash_executor(self):
        ctx = make_ctx()
        await ApollaReviewer().run(make_task("/repo"), ctx)
        # Au moins les appels git (diff, branch, commit_msg, stat) + mkdir
        assert ctx.tools.call.await_count >= 5

    @pytest.mark.asyncio
    async def test_calls_file_io_write(self):
        ctx = make_ctx()
        await ApollaReviewer().run(make_task("/repo"), ctx)
        calls = [c for c in ctx.tools.call.call_args_list if c.args[0] == "file_io"]
        write_calls = [c for c in calls if c.args[1].get("action") == "write"]
        assert len(write_calls) == 1

    @pytest.mark.asyncio
    async def test_writes_to_apollia_reviews(self):
        ctx = make_ctx()
        await ApollaReviewer().run(make_task("/my/repo"), ctx)
        calls = ctx.tools.call.call_args_list
        write_call = next(
            c for c in calls
            if c.args[0] == "file_io" and c.args[1].get("action") == "write"
        )
        assert ".apollia/reviews" in write_call.args[1]["path"]

    @pytest.mark.asyncio
    async def test_report_contains_branch(self):
        ctx = make_ctx(git_stdout="feature/my-branch")
        result = await ApollaReviewer().run(make_task("/repo"), ctx)
        assert "feature/my-branch" in result["output"][0]["text"]

    @pytest.mark.asyncio
    async def test_records_in_memory(self):
        ctx = make_ctx(has_memory=True)
        await ApollaReviewer().run(make_task("/repo"), ctx)
        ctx.memory.record.assert_awaited_once()

    @pytest.mark.asyncio
    async def test_no_memory_call_when_none(self):
        ctx = make_ctx(has_memory=False)
        await ApollaReviewer().run(make_task("/repo"), ctx)
        # ctx.memory est None - aucune AttributeError ne doit être levée

    @pytest.mark.asyncio
    async def test_footer_tier0_when_no_llm(self):
        ctx = make_ctx(has_llm=False)
        result = await ApollaReviewer().run(make_task("/repo"), ctx)
        assert "Tier 0" in result["output"][0]["text"]


# ---------------------------------------------------------------------------
# Tests run() - enrichissement LLM (Palier 1)
# ---------------------------------------------------------------------------

class TestRunTier1:
    @pytest.mark.asyncio
    async def test_llm_chat_called_when_backend_available(self):
        ctx = make_ctx(has_llm=True)
        await ApollaReviewer().run(make_task("/repo"), ctx)
        ctx.llm.chat.assert_awaited_once()

    @pytest.mark.asyncio
    async def test_llm_not_called_when_none(self):
        ctx = make_ctx(has_llm=False)
        await ApollaReviewer().run(make_task("/repo"), ctx)
        # ctx.llm est None - pas d'appel

    @pytest.mark.asyncio
    async def test_footer_tier1_when_llm(self):
        ctx = make_ctx(has_llm=True)
        result = await ApollaReviewer().run(make_task("/repo"), ctx)
        assert "Tier 1" in result["output"][0]["text"]

    @pytest.mark.asyncio
    async def test_llm_response_in_report(self):
        ctx = make_ctx(has_llm=True)
        result = await ApollaReviewer().run(make_task("/repo"), ctx)
        assert "Aucun problème majeur" in result["output"][0]["text"]


# ---------------------------------------------------------------------------
# Tests _build_static_section
# ---------------------------------------------------------------------------

class TestStaticSection:
    def test_no_issues(self):
        section = ApollaReviewer()._build_static_section("diff", "", "")
        assert "No static issues" in section

    def test_detects_todo(self):
        section = ApollaReviewer()._build_static_section(
            "diff", "+// TODO: fix this", ""
        )
        assert "Risky patterns" in section
        assert "TODO" in section

    def test_detects_large_diff(self):
        large_diff = "\n+" * 350
        section = ApollaReviewer()._build_static_section(large_diff, "", "")
        assert "Large diff" in section

    def test_detects_no_test_file(self):
        missing = "NO_TEST: src/engine.rs\n"
        section = ApollaReviewer()._build_static_section("diff", "", missing)
        assert "engine.rs" in section

    def test_small_clean_diff_passes(self):
        diff = "\n+" * 10
        section = ApollaReviewer()._build_static_section(diff, "", "")
        assert "No static issues" in section


# ---------------------------------------------------------------------------
# Tests _build_llm_section
# ---------------------------------------------------------------------------

class TestLlmSection:
    @pytest.mark.asyncio
    async def test_notice_when_no_backend(self):
        ctx = make_ctx(has_llm=False)
        section = await ApollaReviewer()._build_llm_section(ctx, "main", "fix", "diff")
        assert "not configured" in section

    @pytest.mark.asyncio
    async def test_returns_llm_content_when_available(self):
        ctx = make_ctx(has_llm=True)
        section = await ApollaReviewer()._build_llm_section(ctx, "main", "fix", "diff")
        assert "Aucun problème majeur" in section

    @pytest.mark.asyncio
    async def test_diff_truncated_at_8000(self):
        ctx = make_ctx(has_llm=True)
        huge_diff = "x" * 20000
        await ApollaReviewer()._build_llm_section(ctx, "main", "fix", huge_diff)
        user_arg = ctx.llm.chat.call_args.kwargs["user"]
        # Le diff tronqué ne doit pas dépasser la limite + mention de troncature
        assert "truncated" in user_arg

    @pytest.mark.asyncio
    async def test_system_prompt_mentions_rust(self):
        ctx = make_ctx(has_llm=True)
        await ApollaReviewer()._build_llm_section(ctx, "main", "fix", "diff")
        system_arg = ctx.llm.chat.call_args.kwargs["system"]
        assert "Rust" in system_arg

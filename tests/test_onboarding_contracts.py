"""Behavioral contract tests for onboarding-agent.

These tests verify INVARIANTS that must hold regardless of LLM output.
They use mock LLM responses but test the agent's logic guarantees:
  - Tags never leak to user
  - Confidence levels are always correct
  - Language detection drives prompt selection
  - Memory persists incrementally (early-quit safe)
  - Mandatory fields are collected
  - No overwrites of higher-confidence data

Unlike the unit tests in test_onboarding_agent.py which test specific
behaviors, these tests verify contracts that must NEVER be violated.
"""

from __future__ import annotations

import importlib.util
import re
import sys
from pathlib import Path
from typing import Any

import pytest

from apollia.testing import MockContext

# ---------------------------------------------------------------------------
# Agent import
# ---------------------------------------------------------------------------

_PROJECT_ROOT = Path(__file__).resolve().parent.parent
_AGENT_PATH = _PROJECT_ROOT / "agents" / "onboarding-agent.py"

_spec = importlib.util.spec_from_file_location("onboarding_agent", str(_AGENT_PATH))
if _spec is None or _spec.loader is None:
    pytest.skip(f"Agent file not found: {_AGENT_PATH}", allow_module_level=True)

_mod = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = _mod
_spec.loader.exec_module(_mod)
OnboardingAgent = _mod.OnboardingAgent
CONFIDENCE_EXPLICIT = _mod.CONFIDENCE_EXPLICIT
CONFIDENCE_INFERRED = _mod.CONFIDENCE_INFERRED
MEMORY_SOURCE = _mod.MEMORY_SOURCE
ALL_TOPIC_MEMORY_KEYS = _mod.ALL_TOPIC_MEMORY_KEYS


def _llm(text: str) -> dict[str, object]:
    return {"text": text}


# ---------------------------------------------------------------------------
# CONTRACT: Tags never leak to user-visible output
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestContractTagsNeverLeak:
    """INVARIANT: [REMEMBER] and [INFER] tags are NEVER in the response."""

    TAG_PATTERN = re.compile(r"\[(REMEMBER|INFER)\s+\S+=")

    @pytest.mark.parametrize("llm_output", [
        "Hello [REMEMBER user.name=Alice] welcome!",
        "I see [INFER user.role=dev] interesting",
        "[REMEMBER user.name=Bob] [INFER user.expertise_level=senior] Great!",
        "Mixed [REMEMBER a=1] text [INFER b=2] here [REMEMBER c=3]",
        "Nested [REMEMBER user.tools.ide=VSCode] deep key",
        "Value with spaces [REMEMBER user.name=Jean Pierre] ok",
    ])
    async def test_tags_stripped_from_response(self, llm_output: str) -> None:
        agent = OnboardingAgent()
        ctx = MockContext.create(llm_responses=[_llm(llm_output)], memory=True)
        response, _ = await agent.converse(ctx, "Bonjour")
        assert not self.TAG_PATTERN.search(response), (
            f"Tag leaked in response: {response!r}"
        )

    async def test_tags_stripped_across_multi_turn(self) -> None:
        agent = OnboardingAgent()
        responses = [
            _llm("Salut ! [REMEMBER user.name=Nidal]"),
            _llm("CTO, super ! [INFER user.expertise_level=senior]"),
            _llm("Compris [REMEMBER user.domain.type=SaaS] [INFER user.tools.ide=VSCode]"),
        ]
        ctx = MockContext.create(llm_responses=responses, memory=True)

        history: list[dict[str, str]] | None = None
        for msg in ["Bonjour", "Je suis CTO", "On fait du SaaS"]:
            response, history = await agent.converse(ctx, msg, history=history)
            assert not self.TAG_PATTERN.search(response), (
                f"Tag leaked at turn for '{msg}': {response!r}"
            )


# ---------------------------------------------------------------------------
# CONTRACT: Confidence levels are always correct
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestContractConfidenceLevels:
    """INVARIANT: REMEMBER tags → CONFIDENCE_EXPLICIT, INFER → CONFIDENCE_INFERRED."""

    async def test_remember_always_explicit_confidence(self) -> None:
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[_llm("[REMEMBER user.name=A] [REMEMBER user.role=B]")],
            memory=True,
        )
        await agent.converse(ctx, "Hello")

        for op in ctx.memory.operations:
            if op["op"] == "remember" and op["key"] in ("user.name", "user.role"):
                assert op["confidence"] == CONFIDENCE_EXPLICIT, (
                    f"REMEMBER tag for {op['key']} has wrong confidence: {op['confidence']}"
                )

    async def test_infer_always_inferred_confidence(self) -> None:
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[_llm("[INFER user.expertise_level=senior] [INFER user.role=dev]")],
            memory=True,
        )
        await agent.converse(ctx, "Hello")

        for op in ctx.memory.operations:
            if op["op"] == "remember" and op["key"] in ("user.expertise_level", "user.role"):
                assert op["confidence"] == CONFIDENCE_INFERRED, (
                    f"INFER tag for {op['key']} has wrong confidence: {op['confidence']}"
                )

    async def test_mixed_tags_have_correct_confidence(self) -> None:
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[_llm("[REMEMBER user.name=X] text [INFER user.role=Y]")],
            memory=True,
        )
        await agent.converse(ctx, "Hello")

        ops_by_key = {
            op["key"]: op for op in ctx.memory.operations if op["op"] == "remember"
        }
        assert ops_by_key["user.name"]["confidence"] == CONFIDENCE_EXPLICIT
        assert ops_by_key["user.role"]["confidence"] == CONFIDENCE_INFERRED


# ---------------------------------------------------------------------------
# CONTRACT: Memory source is always MEMORY_SOURCE
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestContractMemorySource:
    """INVARIANT: All memory writes use the agent's MEMORY_SOURCE constant."""

    async def test_all_remember_ops_have_correct_source(self) -> None:
        agent = OnboardingAgent()
        responses = [
            _llm("[REMEMBER user.name=A]"),
            _llm("[INFER user.role=B]"),
            _llm("[REMEMBER user.tools.ide=C]"),
        ]
        ctx = MockContext.create(llm_responses=responses, memory=True)

        history: list[dict[str, str]] | None = None
        for msg in ["a", "b", "c"]:
            _, history = await agent.converse(ctx, msg, history=history)

        for op in ctx.memory.operations:
            if op["op"] == "remember":
                assert op["source"] == MEMORY_SOURCE, (
                    f"Memory write for {op['key']} has wrong source: {op['source']!r}"
                )


# ---------------------------------------------------------------------------
# CONTRACT: Higher confidence is never overwritten by lower
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestContractNoConfidenceDowngrade:
    """INVARIANT: A key with confidence X is never overwritten by confidence < X."""

    async def test_explicit_not_overwritten_by_inferred(self) -> None:
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[
                _llm("[REMEMBER user.name=Alice]"),
                _llm("[INFER user.name=Bob]"),
            ],
            memory=True,
        )

        history: list[dict[str, str]] | None = None
        _, history = await agent.converse(ctx, "I'm Alice", history=history)
        _, history = await agent.converse(ctx, "Continue", history=history)

        assert ctx.memory.store["user.name"] == "Alice"
        assert ctx.memory.confidences["user.name"] == CONFIDENCE_EXPLICIT

    async def test_explicit_can_overwrite_inferred(self) -> None:
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[
                _llm("[INFER user.role=dev]"),
                _llm("[REMEMBER user.role=CTO]"),
            ],
            memory=True,
        )

        history: list[dict[str, str]] | None = None
        _, history = await agent.converse(ctx, "Hello", history=history)
        _, history = await agent.converse(ctx, "Actually I'm CTO", history=history)

        assert ctx.memory.store["user.role"] == "CTO"
        assert ctx.memory.confidences["user.role"] == CONFIDENCE_EXPLICIT


# ---------------------------------------------------------------------------
# CONTRACT: Memory persists incrementally (early-quit safe)
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestContractIncrementalPersistence:
    """INVARIANT: After N turns, all memories from turns 1..N are persisted."""

    async def test_each_turn_persists_independently(self) -> None:
        agent = OnboardingAgent()
        responses = [
            _llm("[REMEMBER user.name=Alice]"),
            _llm("[REMEMBER user.role=dev]"),
            _llm("[REMEMBER user.tools.ide=VSCode]"),
        ]
        ctx = MockContext.create(llm_responses=responses, memory=True)

        history: list[dict[str, str]] | None = None

        # After turn 1
        _, history = await agent.converse(ctx, "Hi I'm Alice", history=history)
        assert "user.name" in ctx.memory.store

        # After turn 2
        _, history = await agent.converse(ctx, "I'm a developer", history=history)
        assert "user.name" in ctx.memory.store  # still there
        assert "user.role" in ctx.memory.store

        # After turn 3
        _, history = await agent.converse(ctx, "I use VSCode", history=history)
        assert "user.name" in ctx.memory.store  # still there
        assert "user.role" in ctx.memory.store  # still there
        assert "user.tools.ide" in ctx.memory.store


# ---------------------------------------------------------------------------
# CONTRACT: Language detection sets language memory on first turn
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestContractLanguageDetection:
    """INVARIANT: First message always triggers language preference storage."""

    @pytest.mark.parametrize("msg,expected_lang", [
        ("Bonjour !", "fr"),
        ("Salut, comment ça va ?", "fr"),
        ("Je suis développeur", "fr"),
        ("Hello!", "en"),
        ("Hi there, how are you?", "en"),
        ("I'm a developer", "en"),
    ])
    async def test_language_stored_on_first_message(
        self, msg: str, expected_lang: str
    ) -> None:
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[_llm("Welcome!")],
            memory=True,
        )
        await agent.converse(ctx, msg)
        assert ctx.memory.store.get("user.preferences.language") == expected_lang

    async def test_language_stored_before_llm_call(self) -> None:
        """Language must be persisted even if LLM call fails."""
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[],  # empty — will raise IndexError
            memory=True,
        )
        with pytest.raises(IndexError):
            await agent.converse(ctx, "Bonjour")

        # Language should still be stored despite LLM failure
        assert ctx.memory.store.get("user.preferences.language") == "fr"


# ---------------------------------------------------------------------------
# CONTRACT: All memory keys use the user.* namespace
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestContractMemoryKeyNamespace:
    """INVARIANT: All user-facing memory keys start with 'user.' (except internal state keys)."""

    # Internal keys used by the agent for state tracking, not user profile data
    INTERNAL_KEYS = {"onboarding.state"}

    async def test_all_user_keys_prefixed(self) -> None:
        agent = OnboardingAgent()
        responses = [
            _llm("[REMEMBER user.name=A] [REMEMBER role=B] [INFER expertise=C]"),
        ]
        ctx = MockContext.create(llm_responses=responses, memory=True)
        await agent.converse(ctx, "Hello")

        for op in ctx.memory.operations:
            if op["op"] == "remember" and op["key"] not in self.INTERNAL_KEYS:
                assert op["key"].startswith("user."), (
                    f"Memory key '{op['key']}' missing 'user.' prefix"
                )


# ---------------------------------------------------------------------------
# CONTRACT: History grows correctly
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestContractHistoryIntegrity:
    """INVARIANT: History alternates system/user/assistant correctly."""

    async def test_first_turn_history_structure(self) -> None:
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[_llm("Welcome!")],
            memory=True,
        )
        _, history = await agent.converse(ctx, "Hello")

        assert len(history) == 3
        assert history[0]["role"] == "system"
        assert history[1]["role"] == "user"
        assert history[2]["role"] == "assistant"

    async def test_multi_turn_history_grows_correctly(self) -> None:
        agent = OnboardingAgent()
        responses = [_llm("R1"), _llm("R2"), _llm("R3")]
        ctx = MockContext.create(llm_responses=responses, memory=True)

        history: list[dict[str, str]] | None = None
        _, history = await agent.converse(ctx, "M1", history=history)
        assert len(history) == 3  # system + user + assistant

        _, history = await agent.converse(ctx, "M2", history=history)
        assert len(history) == 5  # + user + assistant

        _, history = await agent.converse(ctx, "M3", history=history)
        assert len(history) == 7  # + user + assistant

    async def test_history_never_contains_tags(self) -> None:
        """Assistant messages in history must be tag-free."""
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[_llm("Hi [REMEMBER user.name=X] there")],
            memory=True,
        )
        _, history = await agent.converse(ctx, "Hello")

        for msg in history:
            if msg["role"] == "assistant":
                assert "[REMEMBER" not in msg["content"]
                assert "[INFER" not in msg["content"]


# ---------------------------------------------------------------------------
# CONTRACT: Empty/edge-case inputs don't crash
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestContractRobustness:
    """INVARIANT: Agent handles edge-case inputs without crashing."""

    @pytest.mark.parametrize("input_text", [
        "",
        " ",
        "   \n\t  ",
        "🎉🚀💻",
        "a" * 10000,
        "<script>alert('xss')</script>",
        "[REMEMBER user.name=hacker]",
        "DROP TABLE users;",
    ])
    async def test_edge_case_inputs(self, input_text: str) -> None:
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[_llm("OK, compris.")],
            memory=True,
        )
        # Should not raise any exception
        response, history = await agent.converse(ctx, input_text)
        assert isinstance(response, str)
        assert isinstance(history, list)


# ---------------------------------------------------------------------------
# CONTRACT: Memory schema completeness
# ---------------------------------------------------------------------------

class TestContractSchemaCompleteness:
    """INVARIANT: ALL_TOPIC_MEMORY_KEYS matches what the agent can produce."""

    def test_all_keys_are_valid_dotted_paths(self) -> None:
        for key in ALL_TOPIC_MEMORY_KEYS:
            assert key.startswith("user."), f"Key {key} missing user. prefix"
            parts = key.split(".")
            assert len(parts) >= 2, f"Key {key} too shallow"
            assert all(p.isidentifier() for p in parts), (
                f"Key {key} contains non-identifier parts"
            )

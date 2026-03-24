"""Tests for onboarding-agent using Apollia SDK testing utilities.

Validates that the onboarding agent:
- Returns a well-formed manifest
- Produces a welcoming first message
- Persists information to memory via remember()
- Preserves memory when the user quits early
"""

from __future__ import annotations

import importlib.util
from pathlib import Path
from typing import Any

import pytest

from apollia.testing import MockContext
from apollia.testing.assertions import assert_result_completed

# ---------------------------------------------------------------------------
# Agent import (filename uses hyphens — requires importlib)
# ---------------------------------------------------------------------------

_PROJECT_ROOT = Path(__file__).resolve().parent.parent
_AGENT_PATH = _PROJECT_ROOT / "agents" / "onboarding-agent.py"

_spec = importlib.util.spec_from_file_location("onboarding_agent", str(_AGENT_PATH))
if _spec is None or _spec.loader is None:
    pytest.skip(
        f"Agent file not found: {_AGENT_PATH}",
        allow_module_level=True,
    )

_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_mod)
OnboardingAgent = _mod.OnboardingAgent
_detect_language = _mod._detect_language
_extract_remember_tags = _mod._extract_remember_tags
_strip_remember_tags = _mod._strip_remember_tags
MEMORY_SOURCE = _mod.MEMORY_SOURCE


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _llm_response(text: str) -> dict[str, object]:
    """Build a mock LLM response dict."""
    return {"text": text}


def _make_llm_responses(texts: list[str]) -> list[dict[str, object]]:
    """Build a list of mock LLM responses for multi-turn conversations."""
    return [_llm_response(t) for t in texts]


# ---------------------------------------------------------------------------
# Manifest
# ---------------------------------------------------------------------------

class TestManifest:
    """Verify the agent manifest is well-formed."""

    def test_manifest_has_required_fields(self) -> None:
        """GIVEN an OnboardingAgent WHEN manifest() THEN required fields present."""
        m = OnboardingAgent().manifest()
        assert m["name"] == "onboarding-agent"
        assert "version" in m
        assert m["execution_mode"] == "conversational"
        assert m["tools_required"] == []
        assert "memory_namespace" in m

    def test_manifest_has_no_dangerous_tools(self) -> None:
        """GIVEN an OnboardingAgent WHEN manifest() THEN no dangerous tools."""
        m = OnboardingAgent().manifest()
        assert m["dangerous_tools_allowed"] is False


# ---------------------------------------------------------------------------
# First message
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestFirstMessage:
    """Verify the first interaction produces a welcome + open question."""

    async def test_french_first_message(self) -> None:
        """GIVEN 'Bonjour' WHEN converse() THEN response is non-empty."""
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[_llm_response(
                "Bienvenue sur Apollia OS ! Je suis ravi de faire ta "
                "connaissance. Pour commencer, quel est ton prénom et "
                "ton rôle ? [REMEMBER user.name=utilisateur]"
            )],
            memory=True,
        )
        response, history = await agent.converse(ctx, "Bonjour !")

        assert len(response) > 0
        assert "[REMEMBER" not in response
        assert len(history) == 3  # system + user + assistant

    async def test_english_first_message(self) -> None:
        """GIVEN 'Hello' WHEN converse() THEN English system prompt used."""
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[_llm_response(
                "Welcome to Apollia OS! I'm excited to get to know you. "
                "What's your name and what do you do?"
            )],
            memory=True,
        )
        response, history = await agent.converse(ctx, "Hello there!")

        assert len(response) > 0
        assert history[0]["role"] == "system"
        assert "onboarding assistant" in history[0]["content"].lower()


# ---------------------------------------------------------------------------
# Memory persistence
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestMemoryPersistence:
    """Verify that information is persisted via ctx.memory.remember()."""

    async def test_five_turn_conversation_persists_memories(self) -> None:
        """GIVEN a 5-turn conversation WHEN inspecting memory THEN >= 3 keys."""
        agent = OnboardingAgent()
        responses = _make_llm_responses([
            "Enchanté ! Quel est ton prénom ? [REMEMBER user.name=Nidal]",
            "Nidal, super ! Et quel est ton rôle ? [REMEMBER user.role=CTO]",
            "CTO, impressionnant ! Tu travailles sur quel type de projets ? "
            "[REMEMBER user.expertise_level=senior]",
            "Des projets SaaS, intéressant. Quel IDE utilises-tu ? "
            "[REMEMBER user.domain.type=SaaS B2B]",
            "VSCode, bon choix ! Et quels workflows aimerais-tu automatiser ? "
            "[REMEMBER user.tools.ide=VSCode]",
        ])
        ctx = MockContext.create(llm_responses=responses, memory=True)

        messages = [
            "Salut !",
            "Je m'appelle Nidal",
            "Je suis CTO d'une startup",
            "On fait du SaaS B2B",
            "J'utilise VSCode principalement",
        ]

        history: list[dict[str, str]] | None = None
        for msg in messages:
            _, history = await agent.converse(ctx, msg, history=history)

        remember_ops = [
            op for op in ctx.memory.operations if op["op"] == "remember"
        ]
        unique_keys = {op["key"] for op in remember_ops}

        assert len(unique_keys) >= 3
        assert all(
            op["source"] == MEMORY_SOURCE
            for op in remember_ops
        )

    async def test_memory_keys_follow_hierarchical_format(self) -> None:
        """GIVEN REMEMBER tags WHEN extracted THEN keys start with 'user.'."""
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[_llm_response(
                "Bienvenue ! [REMEMBER user.name=Alice] "
                "[REMEMBER user.role=developer]"
            )],
            memory=True,
        )
        await agent.converse(ctx, "Bonjour, je suis Alice")

        remember_ops = [
            op for op in ctx.memory.operations
            if op["op"] == "remember" and op["key"].startswith("user.")
        ]
        assert len(remember_ops) >= 2


# ---------------------------------------------------------------------------
# Quit preserves memory
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestQuitPreservesMemory:
    """Verify quitting early does not lose already-collected data."""

    async def test_quit_after_two_messages_preserves_memory(self) -> None:
        """GIVEN 2 messages then quit WHEN checking memory THEN data present."""
        agent = OnboardingAgent()
        responses = _make_llm_responses([
            "Salut ! Comment tu t'appelles ? [REMEMBER user.preferences.language=fr]",
            "Nidal, enchanté ! [REMEMBER user.name=Nidal]",
        ])
        ctx = MockContext.create(llm_responses=responses, memory=True)

        history: list[dict[str, str]] | None = None
        _, history = await agent.converse(ctx, "Bonjour", history=history)
        _, history = await agent.converse(ctx, "Nidal", history=history)

        assert len(ctx.memory.store) > 0
        remember_ops = [
            op for op in ctx.memory.operations if op["op"] == "remember"
        ]
        assert len(remember_ops) >= 2

    async def test_single_message_still_persists_language(self) -> None:
        """GIVEN a single message WHEN quit THEN language preference stored."""
        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[_llm_response("Bienvenue !")],
            memory=True,
        )
        await agent.converse(ctx, "Bonjour")

        assert "user.preferences.language" in ctx.memory.store
        assert ctx.memory.store["user.preferences.language"] == "fr"


# ---------------------------------------------------------------------------
# Language detection
# ---------------------------------------------------------------------------

class TestLanguageDetection:
    """Verify the language detection heuristic."""

    def test_french_detected(self) -> None:
        """GIVEN French text WHEN _detect_language THEN 'fr'."""
        assert _detect_language("Bonjour, comment ça va ?") == "fr"
        assert _detect_language("Salut !") == "fr"
        assert _detect_language("Je suis développeur") == "fr"

    def test_english_detected(self) -> None:
        """GIVEN English text WHEN _detect_language THEN 'en'."""
        assert _detect_language("Hello, how are you?") == "en"
        assert _detect_language("I'm a developer") == "en"
        assert _detect_language("Hi there") == "en"


# ---------------------------------------------------------------------------
# REMEMBER tag extraction
# ---------------------------------------------------------------------------

class TestRememberTagExtraction:
    """Verify the [REMEMBER key=value] parsing logic."""

    def test_extracts_single_tag(self) -> None:
        """GIVEN text with one REMEMBER tag WHEN extracted THEN one pair."""
        pairs = _extract_remember_tags("Hello [REMEMBER user.name=Alice] world")
        assert len(pairs) == 1
        assert pairs[0] == ("user.name", "Alice")

    def test_extracts_multiple_tags(self) -> None:
        """GIVEN text with two REMEMBER tags WHEN extracted THEN two pairs."""
        text = "[REMEMBER user.name=Bob] and [REMEMBER user.role=dev]"
        pairs = _extract_remember_tags(text)
        assert len(pairs) == 2
        keys = {k for k, _ in pairs}
        assert "user.name" in keys
        assert "user.role" in keys

    def test_adds_prefix_if_missing(self) -> None:
        """GIVEN a key without 'user.' prefix WHEN extracted THEN prefix added."""
        pairs = _extract_remember_tags("[REMEMBER name=Charlie]")
        assert pairs[0][0] == "user.name"

    def test_strips_tags_from_output(self) -> None:
        """GIVEN text with REMEMBER tags WHEN stripped THEN tags removed."""
        text = "Hello [REMEMBER user.name=Alice] world"
        result = _strip_remember_tags(text)
        assert "[REMEMBER" not in result
        assert "Hello" in result
        assert "world" in result

    def test_no_tags_returns_empty(self) -> None:
        """GIVEN text without REMEMBER tags WHEN extracted THEN empty list."""
        pairs = _extract_remember_tags("Just a normal response")
        assert pairs == []


# ---------------------------------------------------------------------------
# run() entry point
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestRunEntryPoint:
    """Verify the run() method integrates correctly."""

    async def test_run_returns_completed(self) -> None:
        """GIVEN valid input WHEN run() THEN AIPResult.completed."""
        from unittest.mock import MagicMock

        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[_llm_response("Bienvenue sur Apollia OS !")],
            memory=True,
        )
        task = MagicMock()
        task.input = MagicMock()
        task.input.text = "Bonjour"

        result = await agent.run(task, ctx)
        assert_result_completed(result)

    async def test_run_no_input_returns_failed(self) -> None:
        """GIVEN no input WHEN run() THEN AIPResult.failed."""
        from unittest.mock import MagicMock

        from apollia.testing.assertions import assert_result_failed

        agent = OnboardingAgent()
        ctx = MockContext.create(
            llm_responses=[_llm_response("ignored")],
            memory=True,
        )
        task = MagicMock(spec=[])

        result = await agent.run(task, ctx)
        assert_result_failed(result, code="NO_INPUT")

    async def test_run_no_llm_raises(self) -> None:
        """GIVEN no LLM WHEN run() THEN RuntimeError."""
        from unittest.mock import MagicMock

        agent = OnboardingAgent()
        ctx = MockContext.create(memory=True)
        task = MagicMock()
        task.input = MagicMock()
        task.input.text = "Hello"

        with pytest.raises(RuntimeError, match="requires ctx.llm"):
            await agent.run(task, ctx)

"""Tests for onboarding-agent using Apollia SDK testing utilities.

Validates that the onboarding agent:
- Returns a well-formed manifest
- Produces a welcoming first message
- Persists information to memory via remember()
- Preserves memory when the user quits early
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path
from typing import Any

import pytest

from apollia.testing import MockContext
from apollia.testing.assertions import assert_result_completed

# ---------------------------------------------------------------------------
# Agent import (filename uses hyphens - requires importlib)
# ---------------------------------------------------------------------------

_PROJECT_ROOT = Path(__file__).resolve().parent.parent
_AGENT_PATH = _PROJECT_ROOT / "agents" / "system" / "onboarding-agent" / "agent.py"

_spec = importlib.util.spec_from_file_location("onboarding_agent", str(_AGENT_PATH))
if _spec is None or _spec.loader is None:
    pytest.skip(
        f"Agent file not found: {_AGENT_PATH}",
        allow_module_level=True,
    )

_mod = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = _mod
_spec.loader.exec_module(_mod)
OnboardingAgent = _mod.OnboardingAgent
_detect_language = _mod._detect_language
_extract_remember_tags = _mod._extract_remember_tags
_extract_infer_tags = _mod._extract_infer_tags
_strip_remember_tags = _mod._strip_remember_tags
persist_insight = _mod.persist_insight
MEMORY_SOURCE = _mod.MEMORY_SOURCE
CONFIDENCE_EXPLICIT = _mod.CONFIDENCE_EXPLICIT
CONFIDENCE_INFERRED = _mod.CONFIDENCE_INFERRED
ONBOARDING_MEMORY_SCHEMA = _mod.ONBOARDING_MEMORY_SCHEMA
TOPIC_GUIDES = _mod.TOPIC_GUIDES
ALL_TOPIC_MEMORY_KEYS = _mod.ALL_TOPIC_MEMORY_KEYS
topic_for_memory_key = _mod.topic_for_memory_key
TopicGuide = _mod.TopicGuide


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


# ---------------------------------------------------------------------------
# Topic guides - structure validation
# ---------------------------------------------------------------------------

class TestTopicGuideStructure:
    """Verify that topic guide data structures are well-formed."""

    def test_five_topic_guides_defined(self) -> None:
        """GIVEN TOPIC_GUIDES WHEN inspected THEN exactly 5 topics."""
        assert len(TOPIC_GUIDES) == 5
        expected = {"identity", "preferences", "tools", "domain", "agents"}
        assert set(TOPIC_GUIDES.keys()) == expected

    def test_each_topic_has_memory_keys(self) -> None:
        """GIVEN each TopicGuide WHEN inspected THEN memory_keys non-empty."""
        for name, guide in TOPIC_GUIDES.items():
            assert len(guide.memory_keys) > 0, f"Topic '{name}' has no memory keys"
            for key in guide.memory_keys:
                assert key.startswith("user."), (
                    f"Topic '{name}' key '{key}' must start with 'user.'"
                )

    def test_all_topic_memory_keys_aggregated(self) -> None:
        """GIVEN ALL_TOPIC_MEMORY_KEYS WHEN inspected THEN covers all topics."""
        for guide in TOPIC_GUIDES.values():
            for key in guide.memory_keys:
                assert key in ALL_TOPIC_MEMORY_KEYS

    def test_topic_for_memory_key_resolves(self) -> None:
        """GIVEN a known memory key WHEN topic_for_memory_key THEN correct topic."""
        assert topic_for_memory_key("user.name") == "identity"
        assert topic_for_memory_key("user.role") == "identity"
        assert topic_for_memory_key("user.preferences.verbosity") == "preferences"
        assert topic_for_memory_key("user.tools.ide") == "tools"
        assert topic_for_memory_key("user.domain.stack") == "domain"
        assert topic_for_memory_key("user.agents.workflows") == "agents"
        assert topic_for_memory_key("unknown.key") is None

    def test_system_prompt_contains_topic_memory_keys(self) -> None:
        """GIVEN the system prompts WHEN inspected THEN contain all memory keys."""
        agent = OnboardingAgent()
        prompt_fr = _mod._SYSTEM_PROMPT_FR
        prompt_en = _mod._SYSTEM_PROMPT_EN

        for guide in TOPIC_GUIDES.values():
            for key in guide.memory_keys:
                assert key in prompt_fr, (
                    f"Key '{key}' missing from French system prompt"
                )
                assert key in prompt_en, (
                    f"Key '{key}' missing from English system prompt"
                )


# ---------------------------------------------------------------------------
# Topic coverage in conversations
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestTopicsCoverage:
    """Verify that a multi-turn conversation explores multiple topics."""

    async def test_fifteen_message_conversation_covers_three_topics(self) -> None:
        """GIVEN a 15-message conversation WHEN memory inspected THEN >= 3 topics."""
        agent = OnboardingAgent()
        llm_texts = [
            # Turn 1 - identity
            "Enchanté ! Comment tu t'appelles ? [REMEMBER user.name=Nidal]",
            # Turn 2 - identity
            "Nidal, super ! Et tu fais quoi dans la vie ? [REMEMBER user.role=CTO]",
            # Turn 3 - identity
            "CTO, beau parcours ! Tu te considères senior ? "
            "[REMEMBER user.expertise_level=senior]",
            # Turn 4 - domain
            "Intéressant ! Tu travailles sur quels types de projets ? "
            "[REMEMBER user.domain.type=SaaS B2B]",
            # Turn 5 - domain
            "Du SaaS B2B, c'est passionnant. C'est quoi ta stack ? "
            "[REMEMBER user.domain.stack=Rust + Python]",
            # Turn 6 - tools
            "Rust et Python, combo puissant ! Tu utilises quel éditeur ? "
            "[REMEMBER user.tools.ide=VSCode]",
            # Turn 7 - tools
            "VSCode, classique ! Tu as des outils CLI préférés ? "
            "[REMEMBER user.tools.cli_favorites=ripgrep, fd, jq]",
            # Turn 8 - preferences
            "Bon à savoir ! Tu préfères des réponses détaillées ou concises ? "
            "[REMEMBER user.preferences.verbosity=concise]",
            # Turn 9 - agents
            "Noté ! Tu as des tâches répétitives à automatiser ? "
            "[REMEMBER user.agents.pain_points=code review triage]",
            # Turn 10 - agents
            "Je vois, la review de code. Tu imagines quoi d'autre ? "
            "[REMEMBER user.agents.workflows=PR review automation]",
            # Turn 11 - domain
            "Et des contraintes particulières ? Sécurité, compliance ? "
            "[REMEMBER user.domain.constraints=SOC2]",
            # Turn 12 - preferences
            "Compris pour SOC2. En quelle langue tu préfères qu'on échange ? "
            "[REMEMBER user.preferences.format=markdown]",
            # Turn 13 - tools
            "Tu utilises quel terminal au quotidien ? "
            "[REMEMBER user.tools.terminal=kitty]",
            # Turn 14 - identity
            "Et tu parles quelles langues ? "
            "[REMEMBER user.languages=fr, en]",
            # Turn 15 - agents
            "On a bien avancé ! Tu as des attentes particulières ? "
            "[REMEMBER user.agents.expectations=proactive suggestions]",
        ]
        responses = _make_llm_responses(llm_texts)
        ctx = MockContext.create(llm_responses=responses, memory=True)

        user_messages = [
            "Salut !",
            "Je m'appelle Nidal",
            "CTO et cofondateur",
            "Oui, senior depuis longtemps",
            "Du SaaS B2B pour l'IA",
            "Rust et Python",
            "VSCode avec quelques extensions",
            "ripgrep, fd, jq surtout",
            "Concis, je préfère aller vite",
            "Le triage des code reviews",
            "Automatiser les PR reviews",
            "On est SOC2 compliant",
            "Markdown c'est parfait",
            "Kitty terminal",
            "Français et anglais",
        ]

        history: list[dict[str, str]] | None = None
        for msg in user_messages:
            _, history = await agent.converse(ctx, msg, history=history)

        remember_ops = [
            op for op in ctx.memory.operations if op["op"] == "remember"
        ]
        topics_covered: set[str] = set()
        for op in remember_ops:
            topic = topic_for_memory_key(op["key"])
            if topic is not None:
                topics_covered.add(topic)

        assert len(topics_covered) >= 3, (
            f"Expected >= 3 topics covered, got {len(topics_covered)}: "
            f"{topics_covered}"
        )


# ---------------------------------------------------------------------------
# Topic adaptation
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestTopicAdaptation:
    """Verify that LLM responses use topic-appropriate memory keys."""

    async def test_python_dev_profile_uses_python_tools_keys(self) -> None:
        """GIVEN a Python backend dev WHEN tools explored THEN Python-relevant keys."""
        agent = OnboardingAgent()
        llm_texts = [
            "Enchanté ! [REMEMBER user.name=Alice]",
            "Dev Python backend, super ! [REMEMBER user.role=developer] "
            "[REMEMBER user.expertise_level=senior]",
            "Tu utilises pip ou poetry ? [REMEMBER user.domain.stack=Python backend]",
            "Poetry, bon choix ! Et ton IDE ? [REMEMBER user.tools.ide=PyCharm]",
            "Tu utilises pytest pour les tests ? "
            "[REMEMBER user.tools.cli_favorites=poetry, pytest, black]",
        ]
        responses = _make_llm_responses(llm_texts)
        ctx = MockContext.create(llm_responses=responses, memory=True)

        user_messages = [
            "Hello!",
            "I'm Alice, Python backend developer",
            "Senior, 8 years experience",
            "I use Poetry for everything",
            "PyCharm mostly",
        ]

        history: list[dict[str, str]] | None = None
        for msg in user_messages:
            _, history = await agent.converse(ctx, msg, history=history)

        remember_ops = [
            op for op in ctx.memory.operations if op["op"] == "remember"
        ]
        remembered_keys = {op["key"] for op in remember_ops}
        remembered_values = {
            op["value"] for op in remember_ops if op.get("value")
        }

        assert "user.tools.ide" in remembered_keys
        assert "user.domain.stack" in remembered_keys
        all_values_str = " ".join(remembered_values).lower()
        assert "python" in all_values_str or "pycharm" in all_values_str

    async def test_designer_profile_differs_from_developer(self) -> None:
        """GIVEN a junior designer WHEN memory keys inspected THEN different from dev."""
        agent = OnboardingAgent()
        llm_texts = [
            "Bienvenue ! [REMEMBER user.name=Marie]",
            "UX designer, génial ! [REMEMBER user.role=UX designer] "
            "[REMEMBER user.expertise_level=junior]",
            "Tu utilises quels outils de design ? "
            "[REMEMBER user.tools.ide=Figma]",
            "Et tu travailles sur quels types de projets ? "
            "[REMEMBER user.domain.type=mobile apps]",
            "Tu aimerais automatiser quoi ? "
            "[REMEMBER user.agents.pain_points=design handoff]",
        ]
        responses = _make_llm_responses(llm_texts)
        ctx = MockContext.create(llm_responses=responses, memory=True)

        user_messages = [
            "Salut !",
            "Je suis Marie, UX designer junior",
            "Je débute dans le métier",
            "Figma surtout, et un peu Sketch",
            "Des apps mobiles pour une startup",
        ]

        history: list[dict[str, str]] | None = None
        for msg in user_messages:
            _, history = await agent.converse(ctx, msg, history=history)

        remember_ops = [
            op for op in ctx.memory.operations if op["op"] == "remember"
        ]
        remembered = {op["key"]: op["value"] for op in remember_ops}

        assert remembered.get("user.role") == "UX designer"
        assert remembered.get("user.expertise_level") == "junior"
        assert remembered.get("user.tools.ide") == "Figma"

        topics_covered: set[str] = set()
        for key in remembered:
            topic = topic_for_memory_key(key)
            if topic is not None:
                topics_covered.add(topic)
        assert len(topics_covered) >= 3


# ---------------------------------------------------------------------------
# Topic memory keys mapping
# ---------------------------------------------------------------------------

class TestTopicsMemoryKeysMapping:
    """Verify memory keys are properly associated with topics."""

    def test_all_memory_keys_map_to_a_topic(self) -> None:
        """GIVEN ALL_TOPIC_MEMORY_KEYS WHEN mapped THEN each resolves to a topic."""
        for key in ALL_TOPIC_MEMORY_KEYS:
            topic = topic_for_memory_key(key)
            assert topic is not None, f"Key '{key}' does not map to any topic"
            assert topic in TOPIC_GUIDES, f"Key '{key}' maps to unknown topic '{topic}'"

    def test_identity_keys_map_to_identity(self) -> None:
        """GIVEN identity memory keys WHEN mapped THEN all resolve to 'identity'."""
        expected_keys = {"user.name", "user.role", "user.languages", "user.expertise_level"}
        for key in expected_keys:
            assert topic_for_memory_key(key) == "identity"

    def test_tools_keys_map_to_tools(self) -> None:
        """GIVEN tools memory keys WHEN mapped THEN all resolve to 'tools'."""
        expected_keys = {"user.tools.ide", "user.tools.terminal", "user.tools.cli_favorites"}
        for key in expected_keys:
            assert topic_for_memory_key(key) == "tools"

    def test_domain_keys_map_to_domain(self) -> None:
        """GIVEN domain memory keys WHEN mapped THEN all resolve to 'domain'."""
        expected_keys = {"user.domain.type", "user.domain.stack", "user.domain.constraints"}
        for key in expected_keys:
            assert topic_for_memory_key(key) == "domain"

    def test_agents_keys_map_to_agents(self) -> None:
        """GIVEN agents memory keys WHEN mapped THEN all resolve to 'agents'."""
        expected_keys = {
            "user.agents.workflows",
            "user.agents.pain_points",
            "user.agents.expectations",
        }
        for key in expected_keys:
            assert topic_for_memory_key(key) == "agents"

    def test_preferences_keys_map_to_preferences(self) -> None:
        """GIVEN preferences memory keys WHEN mapped THEN all resolve to 'preferences'."""
        expected_keys = {
            "user.preferences.verbosity",
            "user.preferences.format",
            "user.preferences.language",
        }
        for key in expected_keys:
            assert topic_for_memory_key(key) == "preferences"


# ---------------------------------------------------------------------------
# Onboarding persistence - confidence and no-overwrite
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestOnboardingPersistence:
    """Verify confidence-aware persistence for onboarding results."""

    async def test_persist_explicit_confidence(self) -> None:
        """GIVEN explicit user info WHEN persisted THEN confidence=0.9."""
        agent = OnboardingAgent()
        responses = _make_llm_responses([
            "Enchanté Nidal ! [REMEMBER user.name=Nidal]",
        ])
        ctx = MockContext.create(llm_responses=responses, memory=True)

        await agent.converse(ctx, "Bonjour, je suis Nidal")

        remember_ops = [
            op for op in ctx.memory.operations
            if op["op"] == "remember" and op["key"] == "user.name"
        ]
        assert len(remember_ops) == 1
        assert remember_ops[0]["confidence"] == CONFIDENCE_EXPLICIT
        assert remember_ops[0]["source"] == MEMORY_SOURCE

    async def test_persist_inferred_confidence(self) -> None:
        """GIVEN inferred info WHEN persisted THEN confidence=0.5."""
        agent = OnboardingAgent()
        responses = _make_llm_responses([
            "Tu es développeur, je vois ! "
            "[INFER user.expertise_level=senior]",
        ])
        ctx = MockContext.create(llm_responses=responses, memory=True)

        await agent.converse(ctx, "Bonjour")

        inferred_ops = [
            op for op in ctx.memory.operations
            if op["op"] == "remember" and op["key"] == "user.expertise_level"
        ]
        assert len(inferred_ops) == 1
        assert inferred_ops[0]["confidence"] == CONFIDENCE_INFERRED
        assert inferred_ops[0]["source"] == MEMORY_SOURCE

    async def test_recall_after_onboarding(self) -> None:
        """GIVEN a completed onboarding WHEN recall THEN preferences returned."""
        agent = OnboardingAgent()
        responses = _make_llm_responses([
            "Bienvenue ! [REMEMBER user.name=Nidal]",
            "Super ! [REMEMBER user.role=CTO]",
            "Noté ! [REMEMBER user.preferences.verbosity=concise]",
            "D'accord ! [REMEMBER user.preferences.format=markdown]",
            "Compris ! [REMEMBER user.tools.ide=VSCode]",
            "Excellent ! [REMEMBER user.domain.type=SaaS B2B]",
            "Bien vu ! [REMEMBER user.domain.stack=Rust, Python]",
            "C'est noté ! [REMEMBER user.agents.pain_points=code review triage]",
        ])
        ctx = MockContext.create(llm_responses=responses, memory=True)

        messages = [
            "Salut !",
            "Nidal",
            "CTO",
            "Concis",
            "Markdown",
            "VSCode",
            "SaaS B2B",
            "Le triage des code reviews",
        ]

        history: list[dict[str, str]] | None = None
        for msg in messages:
            _, history = await agent.converse(ctx, msg, history=history)

        remember_ops = [
            op for op in ctx.memory.operations
            if op["op"] == "remember" and not op.get("skipped", False)
        ]
        unique_keys = {op["key"] for op in remember_ops}

        assert len(unique_keys) >= 5
        assert all(op["source"] == MEMORY_SOURCE for op in remember_ops)

        pref_value = await ctx.memory.recall("user.preferences.verbosity")
        assert pref_value == "concise"

    async def test_no_overwrite_higher_confidence(self) -> None:
        """GIVEN explicit entry WHEN inferred value arrives THEN original preserved."""
        agent = OnboardingAgent()
        responses = _make_llm_responses([
            "Enchanté Nidal ! [REMEMBER user.name=Nidal]",
            "Hmm, peut-être un autre nom ? [INFER user.name=N.]",
        ])
        ctx = MockContext.create(llm_responses=responses, memory=True)

        history: list[dict[str, str]] | None = None
        _, history = await agent.converse(ctx, "Bonjour, je suis Nidal", history=history)
        _, history = await agent.converse(ctx, "Oui c'est bien mon nom", history=history)

        assert ctx.memory.store["user.name"] == "Nidal"
        assert ctx.memory.confidences["user.name"] == CONFIDENCE_EXPLICIT

        name_ops = [
            op for op in ctx.memory.operations
            if op["op"] == "remember" and op["key"] == "user.name"
        ]
        skipped_ops = [op for op in name_ops if op.get("skipped", False)]
        assert len(skipped_ops) == 1


# ---------------------------------------------------------------------------
# INFER tag extraction
# ---------------------------------------------------------------------------

class TestInferTagExtraction:
    """Verify the [INFER key=value] parsing logic."""

    def test_extracts_infer_tag(self) -> None:
        """GIVEN text with INFER tag WHEN extracted THEN one pair."""
        pairs = _extract_infer_tags("Response [INFER user.expertise_level=senior] here")
        assert len(pairs) == 1
        assert pairs[0] == ("user.expertise_level", "senior")

    def test_infer_tags_stripped(self) -> None:
        """GIVEN text with INFER tags WHEN stripped THEN tags removed."""
        text = "Hello [INFER user.role=dev] world"
        result = _strip_remember_tags(text)
        assert "[INFER" not in result
        assert "Hello" in result
        assert "world" in result

    def test_mixed_remember_and_infer(self) -> None:
        """GIVEN text with both REMEMBER and INFER WHEN extracted THEN both found."""
        text = "[REMEMBER user.name=Alice] likes [INFER user.role=developer]"
        remember_pairs = _extract_remember_tags(text)
        infer_pairs = _extract_infer_tags(text)
        assert len(remember_pairs) == 1
        assert len(infer_pairs) == 1
        assert remember_pairs[0] == ("user.name", "Alice")
        assert infer_pairs[0] == ("user.role", "developer")


# ---------------------------------------------------------------------------
# Onboarding memory schema
# ---------------------------------------------------------------------------

class TestOnboardingMemorySchema:
    """Verify the onboarding memory schema is well-formed."""

    def test_schema_covers_all_topic_keys(self) -> None:
        """GIVEN ONBOARDING_MEMORY_SCHEMA WHEN checked THEN all topic keys present."""
        for key in ALL_TOPIC_MEMORY_KEYS:
            assert key in ONBOARDING_MEMORY_SCHEMA, (
                f"Key '{key}' missing from ONBOARDING_MEMORY_SCHEMA"
            )

    def test_schema_entries_have_topic(self) -> None:
        """GIVEN each schema entry WHEN checked THEN has topic field."""
        for key, meta in ONBOARDING_MEMORY_SCHEMA.items():
            assert "topic" in meta, f"Key '{key}' missing 'topic' field"
            assert "type" in meta, f"Key '{key}' missing 'type' field"

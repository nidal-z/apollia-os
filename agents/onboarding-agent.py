"""onboarding-agent — Conversational onboarding for Apollia OS first-time users.

Guides the user through a natural conversation to learn about their identity,
preferences, tools, domain, and automation goals.  Persists every piece of
information incrementally via ``ctx.memory.remember()`` so the user can leave
at any time without losing data.

Apollia features used:
  - ConversationalAgent inheritance (apollia.agents)
  - Semantic memory persistence (ctx.memory.remember / ctx.memory.recall)
  - LLM-driven dialogue (ctx.llm.complete)
  - Language detection on first message

Quick start:
  apollia-os agent start onboarding-agent
  apollia-os run onboarding-agent --input "Bonjour !"
"""

from __future__ import annotations

import json
from typing import Any

from apollia.agents import AIPResult, ConversationalAgent


# ---------------------------------------------------------------------------
# Memory key constants
# ---------------------------------------------------------------------------

MEMORY_SOURCE: str = "onboarding"

MEMORY_KEY_PREFIX: str = "user."

MEMORY_KEY_LANGUAGE: str = "user.preferences.language"

MEMORY_KEY_ONBOARDING_STATE: str = "onboarding.state"


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------

_SYSTEM_PROMPT_FR = """\
Tu es l'assistant d'onboarding d'Apollia OS. Ton rôle est de faire \
connaissance avec l'utilisateur de manière naturelle et amicale.

Tu explores 5 domaines au fil de la conversation :
- Identité : prénom, rôle professionnel, langues parlées, niveau technique
- Préférences : verbosité des réponses, format préféré, langue d'interface
- Outils : IDE, terminal, CLI favoris, outils de dev quotidiens
- Domaine : types de projets, stack technique, contraintes particulières
- Agents : workflows à automatiser, tâches répétitives

RÈGLES :
- Ne pose JAMAIS une liste de questions. Pose UNE question à la fois.
- Rebondis sur les réponses pour creuser naturellement.
- Adapte tes questions : si l'utilisateur est dev Python, ne demande pas ses outils C++.
- Quand tu apprends quelque chose d'utile, indique-le entre crochets \
[REMEMBER key=valeur] pour que le système le persiste.
- L'utilisateur peut quitter à tout moment. Ne force jamais la conversation.
- Sois chaleureux, concis, et professionnel.
- Commence par te présenter brièvement et poser une première question ouverte.\
"""

_SYSTEM_PROMPT_EN = """\
You are the onboarding assistant for Apollia OS. Your role is to get to know \
the user in a natural, friendly way.

You explore 5 domains during the conversation:
- Identity: first name, professional role, spoken languages, technical level
- Preferences: response verbosity, preferred format, interface language
- Tools: IDE, terminal, favourite CLIs, daily dev tools
- Domain: project types, tech stack, particular constraints
- Agents: workflows to automate, repetitive tasks

RULES:
- NEVER ask a numbered list of questions. Ask ONE question at a time.
- Build on answers to dig deeper naturally.
- Adapt your questions: if the user is a Python dev, don't ask about C++ tools.
- When you learn something useful, indicate it in brackets \
[REMEMBER key=value] so the system persists it.
- The user can quit at any time. Never force the conversation.
- Be warm, concise, and professional.
- Start by briefly introducing yourself and asking one open question.\
"""


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

_FRENCH_MARKERS: frozenset[str] = frozenset((
    "bonjour", "salut", "bonsoir", "coucou", "hey", "allo",
    "je suis", "je m'appelle", "merci", "oui", "non",
    "comment", "quoi", "pourquoi", "bienvenue",
))


def _detect_language(text: str) -> str:
    """Detect whether *text* is likely French or English.

    Returns ``"fr"`` or ``"en"``.
    """
    lower = text.lower()
    tokens = set(lower.split())
    french_hits = sum(1 for marker in _FRENCH_MARKERS if marker in tokens or marker in lower)
    if french_hits >= 1:
        return "fr"
    return "en"


def _extract_remember_tags(text: str) -> list[tuple[str, str]]:
    """Extract ``[REMEMBER key=value]`` pairs from LLM output.

    Returns a list of ``(key, value)`` tuples.  Keys are normalised to
    lower-case with dots.
    """
    pairs: list[tuple[str, str]] = []
    import re
    for match in re.finditer(r"\[REMEMBER\s+([^\]=]+)=([^\]]+)\]", text):
        raw_key = match.group(1).strip().lower().replace(" ", "_")
        value = match.group(2).strip()
        if not raw_key.startswith(MEMORY_KEY_PREFIX):
            raw_key = MEMORY_KEY_PREFIX + raw_key
        pairs.append((raw_key, value))
    return pairs


def _strip_remember_tags(text: str) -> str:
    """Remove ``[REMEMBER ...]`` tags from text shown to the user."""
    import re
    return re.sub(r"\s*\[REMEMBER\s+[^\]]+\]\s*", " ", text).strip()


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------

class OnboardingAgent(ConversationalAgent):
    """Conversational onboarding agent for Apollia OS.

    Guides new users through a natural dialogue covering identity,
    preferences, tools, domain, and automation goals.  Each piece
    of information is persisted immediately via semantic memory.
    """

    SYSTEM_PROMPT = _SYSTEM_PROMPT_FR
    MAX_TURNS = 30
    TEMPERATURE = 0.7

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest."""
        return {
            "name": "onboarding-agent",
            "version": "1.0.0",
            "description": (
                "Agent d'onboarding conversationnel — fait connaissance "
                "avec l'utilisateur de manière naturelle."
            ),
            "execution_mode": "conversational",
            "tools_required": [],
            "tools_optional": [],
            "memory_namespace": "onboarding-agent",
            "max_concurrent_tasks": 1,
            "dangerous_tools_allowed": False,
            "tags": ["onboarding", "conversational"],
        }

    def on_response(self, response: str) -> str:
        """Strip internal REMEMBER tags before showing to the user."""
        return _strip_remember_tags(response)

    async def converse(
        self,
        ctx: Any,
        user_message: str,
        history: list[dict[str, str]] | None = None,
    ) -> tuple[str, list[dict[str, str]]]:
        """Override to handle language detection and memory extraction.

        On the first message (no history), detects the user's language
        and switches the system prompt accordingly.  After each LLM
        response, extracts REMEMBER tags and persists them.
        """
        is_first_message = not history

        if is_first_message:
            lang = _detect_language(user_message)
            self._current_language = lang
            self.SYSTEM_PROMPT = (
                _SYSTEM_PROMPT_FR if lang == "fr" else _SYSTEM_PROMPT_EN
            )
            if ctx.memory is not None:
                await ctx.memory.remember(
                    MEMORY_KEY_LANGUAGE, lang, source=MEMORY_SOURCE,
                )

        if ctx.llm is None:
            raise RuntimeError(
                "OnboardingAgent requires ctx.llm — no LLM backend configured"
            )

        messages: list[dict[str, str]] = list(history) if history else []

        if not messages or messages[0].get("role") != "system":
            messages.insert(0, {"role": "system", "content": self.SYSTEM_PROMPT})

        messages.append({"role": "user", "content": user_message})

        response = await ctx.llm.complete(messages)
        raw_text: str = response.get("text", "")

        pairs = _extract_remember_tags(raw_text)
        if ctx.memory is not None:
            for key, value in pairs:
                await ctx.memory.remember(key, value, source=MEMORY_SOURCE)

        processed_text = self.on_response(raw_text)

        messages.append({"role": "assistant", "content": processed_text})

        if ctx.memory is not None:
            await ctx.memory.record(
                content=f"user: {user_message}\nassistant: {processed_text}",
                importance=0.3,
            )

        if is_first_message and ctx.memory is not None:
            await ctx.memory.remember(
                MEMORY_KEY_ONBOARDING_STATE,
                json.dumps({"started": True, "turns": 1}),
                source=MEMORY_SOURCE,
            )

        return processed_text, messages

    async def run(self, task: Any, ctx: Any) -> AIPResult:
        """Execute the onboarding agent for a single message turn.

        Extracts user text from task input, delegates to ``converse()``,
        and returns the response as ``AIPResult.completed()``.
        """
        if ctx.llm is None:
            raise RuntimeError(
                "OnboardingAgent requires ctx.llm — no LLM backend configured"
            )

        user_message = getattr(task, "input", None)
        if user_message is None:
            return AIPResult.failed("NO_INPUT", "No input provided in task")

        input_text = (
            user_message.text
            if hasattr(user_message, "text")
            else str(user_message)
        )

        response_text, _ = await self.converse(ctx, input_text)
        return AIPResult.completed(response_text)


# ---------------------------------------------------------------------------
# Module-level agent instance (required by the Apollia AIP contract)
# ---------------------------------------------------------------------------

agent = OnboardingAgent()

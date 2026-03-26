"""onboarding-agent — Conversational onboarding for Apollia OS first-time users.

Guides the user through a natural conversation to learn about their identity,
preferences, tools, domain, and automation goals.  Persists every piece of
information incrementally via ``ctx.memory.remember()`` so the user can leave
at any time without losing data.

Each topic is a contextual guide embedded in the system prompt — the LLM
decides when and how to explore topics based on the conversation flow.

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
import re
from dataclasses import dataclass
from typing import Any

from apollia.agents import AIPResult, ConversationalAgent


# ---------------------------------------------------------------------------
# Memory key constants
# ---------------------------------------------------------------------------

MEMORY_SOURCE: str = "onboarding"

MEMORY_KEY_PREFIX: str = "user."

MEMORY_KEY_LANGUAGE: str = "user.preferences.language"

MEMORY_KEY_ONBOARDING_STATE: str = "onboarding.state"

CONFIDENCE_EXPLICIT: float = 0.9
CONFIDENCE_INFERRED: float = 0.5
CONFIDENCE_VALIDATED: float = 0.95

ONBOARDING_MEMORY_SCHEMA: dict[str, dict[str, str]] = {
    "user.name": {"type": "string", "topic": "identity"},
    "user.role": {"type": "string", "topic": "identity"},
    "user.languages": {"type": "list[string]", "topic": "identity"},
    "user.expertise_level": {"type": "string", "topic": "identity"},
    "user.preferences.verbosity": {"type": "string", "topic": "preferences"},
    "user.preferences.format": {"type": "string", "topic": "preferences"},
    "user.preferences.language": {"type": "string", "topic": "preferences"},
    "user.tools.ide": {"type": "string", "topic": "tools"},
    "user.tools.terminal": {"type": "string", "topic": "tools"},
    "user.tools.cli_favorites": {"type": "list[string]", "topic": "tools"},
    "user.tools.package_manager": {"type": "string", "topic": "tools"},
    "user.domain.type": {"type": "string", "topic": "domain"},
    "user.domain.stack": {"type": "list[string]", "topic": "domain"},
    "user.domain.constraints": {"type": "list[string]", "topic": "domain"},
    "user.agents.workflows": {"type": "list[string]", "topic": "agents"},
    "user.agents.pain_points": {"type": "list[string]", "topic": "agents"},
    "user.agents.expectations": {"type": "string", "topic": "agents"},
}


# ---------------------------------------------------------------------------
# Topic guides
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class TopicGuide:
    """Definition of an onboarding topic that the agent explores.

    Each guide provides the LLM with context about what to collect,
    how to adapt questions, and which memory keys to populate.
    """

    name: str
    domain_fr: str
    domain_en: str
    objective_fr: str
    objective_en: str
    memory_keys: tuple[str, ...]
    example_questions_fr: tuple[str, ...]
    example_questions_en: tuple[str, ...]
    adaptation_rules_fr: tuple[str, ...]
    adaptation_rules_en: tuple[str, ...]


TOPIC_IDENTITY = TopicGuide(
    name="identity",
    domain_fr="Identité de l'utilisateur",
    domain_en="User identity",
    objective_fr="Comprendre qui est l'utilisateur",
    objective_en="Understand who the user is",
    memory_keys=(
        "user.name",
        "user.role",
        "user.languages",
        "user.expertise_level",
    ),
    example_questions_fr=(
        "Comment tu t'appelles ?",
        "Quel est ton rôle au quotidien ?",
        "Tu te considères plutôt débutant, intermédiaire, ou senior en dev ?",
    ),
    example_questions_en=(
        "What's your name?",
        "What's your day-to-day role?",
        "Would you consider yourself a beginner, intermediate, or senior dev?",
    ),
    adaptation_rules_fr=(
        "Si l'utilisateur donne son rôle spontanément, ne pas redemander.",
        "Si le prénom est déjà connu, passer à autre chose.",
    ),
    adaptation_rules_en=(
        "If the user mentions their role spontaneously, do not ask again.",
        "If the first name is already known, move on.",
    ),
)

TOPIC_PREFERENCES = TopicGuide(
    name="preferences",
    domain_fr="Préférences d'interaction",
    domain_en="Interaction preferences",
    objective_fr="Adapter le comportement d'Apollia OS",
    objective_en="Adapt Apollia OS behaviour",
    memory_keys=(
        "user.preferences.verbosity",
        "user.preferences.format",
        "user.preferences.language",
    ),
    example_questions_fr=(
        "Tu préfères des réponses détaillées ou aller droit au but ?",
        "Tu veux que je te parle en français ou en anglais ?",
    ),
    example_questions_en=(
        "Do you prefer detailed answers or straight to the point?",
        "Which language would you like me to use?",
    ),
    adaptation_rules_fr=(
        "Observer le style de l'utilisateur : réponses courtes = préfère concis.",
        "Si la langue est déjà détectée, proposer de confirmer plutôt que demander.",
    ),
    adaptation_rules_en=(
        "Observe the user's style: short answers = prefers concise.",
        "If the language is already detected, offer to confirm rather than ask.",
    ),
)

TOPIC_TOOLS = TopicGuide(
    name="tools",
    domain_fr="Outils de développement",
    domain_en="Development tools",
    objective_fr="Connaître l'écosystème de l'utilisateur",
    objective_en="Learn the user's tooling ecosystem",
    memory_keys=(
        "user.tools.ide",
        "user.tools.terminal",
        "user.tools.cli_favorites",
    ),
    example_questions_fr=(
        "Tu utilises quel éditeur de code ?",
        "Tu as des outils CLI que tu ne pourrais pas quitter ?",
    ),
    example_questions_en=(
        "Which code editor do you use?",
        "Are there any CLI tools you couldn't live without?",
    ),
    adaptation_rules_fr=(
        "Si dev Python → demander pip/poetry/conda.",
        "Si dev JS → demander npm/yarn/pnpm.",
        "Ne pas demander des outils d'un écosystème non pertinent.",
    ),
    adaptation_rules_en=(
        "If Python dev → ask about pip/poetry/conda.",
        "If JS dev → ask about npm/yarn/pnpm.",
        "Do not ask about tools from an irrelevant ecosystem.",
    ),
)

TOPIC_DOMAIN = TopicGuide(
    name="domain",
    domain_fr="Contexte professionnel",
    domain_en="Professional context",
    objective_fr="Comprendre les projets et contraintes",
    objective_en="Understand projects and constraints",
    memory_keys=(
        "user.domain.type",
        "user.domain.stack",
        "user.domain.constraints",
    ),
    example_questions_fr=(
        "Tu travailles sur quel genre de projets en ce moment ?",
        "C'est quoi ta stack principale ?",
    ),
    example_questions_en=(
        "What kind of projects are you working on?",
        "What's your main tech stack?",
    ),
    adaptation_rules_fr=(
        "Si SaaS → demander cloud provider.",
        "Si embarqué → demander cibles matérielles.",
    ),
    adaptation_rules_en=(
        "If SaaS → ask about cloud provider.",
        "If embedded → ask about hardware targets.",
    ),
)

TOPIC_AGENTS = TopicGuide(
    name="agents",
    domain_fr="Automatisation souhaitée",
    domain_en="Desired automation",
    objective_fr="Identifier les workflows à automatiser",
    objective_en="Identify workflows to automate",
    memory_keys=(
        "user.agents.workflows",
        "user.agents.pain_points",
        "user.agents.expectations",
    ),
    example_questions_fr=(
        "Tu as des tâches répétitives que tu aimerais automatiser ?",
        "Tu imagines quels genres de choses qu'un agent IA pourrait faire pour toi ?",
    ),
    example_questions_en=(
        "Do you have repetitive tasks you'd like to automate?",
        "What kind of things do you imagine an AI agent could do for you?",
    ),
    adaptation_rules_fr=(
        "Si l'utilisateur ne connaît pas les agents IA, expliquer brièvement.",
        "S'il a déjà de l'expérience, demander quels outils il a essayé.",
    ),
    adaptation_rules_en=(
        "If the user is unfamiliar with AI agents, explain briefly.",
        "If they have experience, ask which tools they've tried.",
    ),
)

TOPIC_GUIDES: dict[str, TopicGuide] = {
    "identity": TOPIC_IDENTITY,
    "preferences": TOPIC_PREFERENCES,
    "tools": TOPIC_TOOLS,
    "domain": TOPIC_DOMAIN,
    "agents": TOPIC_AGENTS,
}

ALL_TOPIC_MEMORY_KEYS: frozenset[str] = frozenset(
    key
    for guide in TOPIC_GUIDES.values()
    for key in guide.memory_keys
)


def topic_for_memory_key(key: str) -> str | None:
    """Return the topic name a memory key belongs to, or ``None``.

    Matches by checking if the key starts with any of the topic's
    declared memory key prefixes.
    """
    for topic_name, guide in TOPIC_GUIDES.items():
        for mk in guide.memory_keys:
            if key == mk or key.startswith(mk + "."):
                return topic_name
    return None


def _build_topic_section_fr(guide: TopicGuide) -> str:
    """Render a single topic guide as a French system prompt section."""
    lines = [
        f"### {guide.domain_fr}",
        f"Objectif : {guide.objective_fr}",
        f"Clés mémoire : {', '.join(guide.memory_keys)}",
        "Questions types :",
    ]
    for q in guide.example_questions_fr:
        lines.append(f"  - \"{q}\"")
    lines.append("Adaptation :")
    for r in guide.adaptation_rules_fr:
        lines.append(f"  - {r}")
    return "\n".join(lines)


def _build_topic_section_en(guide: TopicGuide) -> str:
    """Render a single topic guide as an English system prompt section."""
    lines = [
        f"### {guide.domain_en}",
        f"Objective: {guide.objective_en}",
        f"Memory keys: {', '.join(guide.memory_keys)}",
        "Example questions:",
    ]
    for q in guide.example_questions_en:
        lines.append(f'  - "{q}"')
    lines.append("Adaptation:")
    for r in guide.adaptation_rules_en:
        lines.append(f"  - {r}")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------

_TOPIC_SECTIONS_FR = "\n\n".join(
    _build_topic_section_fr(g) for g in TOPIC_GUIDES.values()
)

_TOPIC_SECTIONS_EN = "\n\n".join(
    _build_topic_section_en(g) for g in TOPIC_GUIDES.values()
)

_SYSTEM_PROMPT_FR = f"""\
Tu es l'assistant d'onboarding d'Apollia OS. Ton rôle est de faire \
connaissance avec l'utilisateur de manière naturelle et amicale.

Tu explores 5 domaines au fil de la conversation. Tu ne suis PAS un ordre \
fixe — tu choisis quand et comment aborder chaque domaine en fonction de ce \
que l'utilisateur te dit. Tu peux revenir sur un domaine si une nouvelle \
information le justifie, ou sauter un domaine si le contexte le rend non \
pertinent.

## Domaines à explorer

{_TOPIC_SECTIONS_FR}

## Règles

- Ne pose JAMAIS une liste de questions. Pose UNE question à la fois.
- Rebondis sur les réponses pour creuser naturellement.
- Adapte tes questions au profil qui se dessine : si l'utilisateur est dev \
Python, ne demande pas ses outils C++.
- Quand tu apprends quelque chose d'utile dit explicitement par l'utilisateur, \
indique-le entre crochets [REMEMBER clé=valeur]. \
Quand tu déduis une information du contexte (ex: l'utilisateur écrit en \
français donc il est probablement francophone), utilise [INFER clé=valeur]. \
Utilise les clés mémoire listées dans chaque domaine.
- L'utilisateur peut quitter à tout moment. Ne force jamais la conversation.
- Sois chaleureux, concis, et professionnel.
- Commence par te présenter brièvement et poser une première question ouverte.\
"""

_SYSTEM_PROMPT_EN = f"""\
You are the onboarding assistant for Apollia OS. Your role is to get to know \
the user in a natural, friendly way.

You explore 5 domains during the conversation. You do NOT follow a fixed \
order — you choose when and how to address each domain based on what the \
user tells you. You can revisit a domain if new information warrants it, or \
skip a domain if the context makes it irrelevant.

## Domains to explore

{_TOPIC_SECTIONS_EN}

## Rules

- NEVER ask a numbered list of questions. Ask ONE question at a time.
- Build on answers to dig deeper naturally.
- Adapt your questions to the emerging profile: if the user is a Python dev, \
don't ask about C++ tools.
- When you learn something useful stated explicitly by the user, indicate it \
in brackets [REMEMBER key=value]. \
When you infer information from context (e.g. the user writes in French so \
they are likely francophone), use [INFER key=value]. \
Use the memory keys listed in each domain.
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
    for match in re.finditer(r"\[REMEMBER\s+([^\]=]+)=([^\]]+)\]", text):
        raw_key = match.group(1).strip().lower().replace(" ", "_")
        value = match.group(2).strip()
        if not raw_key.startswith(MEMORY_KEY_PREFIX):
            raw_key = MEMORY_KEY_PREFIX + raw_key
        pairs.append((raw_key, value))
    return pairs


def _extract_infer_tags(text: str) -> list[tuple[str, str]]:
    """Extract ``[INFER key=value]`` pairs from LLM output.

    Same format as REMEMBER tags but indicates information deduced
    from context rather than explicitly stated by the user.
    """
    pairs: list[tuple[str, str]] = []
    for match in re.finditer(r"\[INFER\s+([^\]=]+)=([^\]]+)\]", text):
        raw_key = match.group(1).strip().lower().replace(" ", "_")
        value = match.group(2).strip()
        if not raw_key.startswith(MEMORY_KEY_PREFIX):
            raw_key = MEMORY_KEY_PREFIX + raw_key
        pairs.append((raw_key, value))
    return pairs


def _strip_remember_tags(text: str) -> str:
    """Remove ``[REMEMBER ...]`` and ``[INFER ...]`` tags from text shown to the user."""
    cleaned = re.sub(r"\s*\[REMEMBER\s+[^\]]+\]\s*", " ", text)
    cleaned = re.sub(r"\s*\[INFER\s+[^\]]+\]\s*", " ", cleaned)
    return cleaned.strip()


async def persist_insight(
    ctx: Any,
    key: str,
    value: str,
    explicit: bool = True,
) -> None:
    """Persist an onboarding insight with the appropriate confidence score.

    Explicit information (the user stated it directly) gets
    ``CONFIDENCE_EXPLICIT`` (0.9).  Inferred information (deduced from
    conversational context) gets ``CONFIDENCE_INFERRED`` (0.5).

    The underlying memory layer skips the write if the key already holds
    a value with strictly higher confidence.
    """
    confidence = CONFIDENCE_EXPLICIT if explicit else CONFIDENCE_INFERRED
    await ctx.memory.remember(
        key=key,
        value=value,
        source=MEMORY_SOURCE,
        confidence=confidence,
    )


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
            "version": "1.2.0",
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
                await persist_insight(
                    ctx, MEMORY_KEY_LANGUAGE, lang, explicit=False,
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
        # LlmResponse is a PyO3 object with .content attribute (not a dict).
        raw_text: str = getattr(response, "content", "") or ""

        explicit_pairs = _extract_remember_tags(raw_text)
        inferred_pairs = _extract_infer_tags(raw_text)
        if ctx.memory is not None:
            for key, value in explicit_pairs:
                await persist_insight(ctx, key, value, explicit=True)
            for key, value in inferred_pairs:
                await persist_insight(ctx, key, value, explicit=False)

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
                confidence=CONFIDENCE_EXPLICIT,
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

        # task is a Python dict (serialised from Rust AIPTask via JSON).
        # task["input"]["parts"][0]["text"] contains the user message.
        task_input = task.get("input") if isinstance(task, dict) else getattr(task, "input", None)
        if task_input is None:
            return AIPResult.failed("NO_INPUT", "No input provided in task")

        # Extract text from the first TextPart in input.parts
        if isinstance(task_input, dict):
            parts = task_input.get("parts", [])
            input_text = parts[0]["text"] if parts else str(task_input)
        elif hasattr(task_input, "parts"):
            parts = task_input.parts
            input_text = parts[0].text if parts else str(task_input)
        elif hasattr(task_input, "text"):
            input_text = task_input.text
        else:
            input_text = str(task_input)

        response_text, _ = await self.converse(ctx, input_text)
        return AIPResult.completed(response_text)


# ---------------------------------------------------------------------------
# Module-level agent instance (required by the Apollia AIP contract)
# ---------------------------------------------------------------------------

agent = OnboardingAgent()

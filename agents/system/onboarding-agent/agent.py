"""onboarding-agent — Conversational onboarding for Apollia OS first-time users.

Guides the user through a progressive dialogue covering business context,
technical profile, automation objectives, and constraints. Every durable fact
is persisted immediately via ``ctx.memory.remember()`` so the user can leave
at any time without losing progress.

================================================================================
PHASE 1 — AUDIT (état avant refonte, v2.0.0 historique)
================================================================================

AUDIT MÉMOIRE :
  Clés exploitées en aval (par crates/apollia-desktop) :
    - user.role          → onboarding.rs:754  (mode operator/builder + mandatory)
    - user.name          → onboarding.rs:753  (mandatory_fields_collected — bloquant)
    - user.expertise     → onboarding.rs:757  (topic identity)
    - user.goals         → onboarding.rs:758  (topic identity)
    - user.domain.*      → onboarding.rs:765  (topic domain + user_memory.rs:238)
    - user.preferences.* → onboarding.rs:761  (topic preferences + Habits view)
    - user.tools.*       → onboarding.rs:763  (topic tools + Habits view)
    - user.agents.*      → onboarding.rs:767  (topic agents + completion gate)
    - onboarding.active_profile → injecté par desktop, lu sur 1er tour
    - onboarding.state          → écrit au 1er tour (started/turns)
  Clés orphelines (déclarées mais ignorées par desktop) :
    - user.preferences.language  (inférée jamais lue côté desktop)
    - user.tools.current vs user.tools.integrations  (redondance — free text + list)
  Clés MANQUANTES bloquantes (attendues côté desktop, jamais écrites) :
    - user.name          ← desktop bloque la complétion sans (onboarding.rs:789-795)
    - user.languages     ← desktop catégorise mais agent n'écrit pas
    - user.industry      ← desktop mappe mais agent écrit user.domain.sector
                            (compatible via préfixe user.domain, donc OK)
  Clés MANDATORY déclarées par l'agent : user.role, user.agents.domains
  Clés MANDATORY attendues par desktop  : user.name, user.role
  ⇒ MISMATCH : l'onboarding « se termine » côté agent mais reste bloqué côté UI.

AUDIT CONVERSATION :
  Topics : 5 (identity / domain / tools / preferences / agents) — 13 clés
  Tours moyens observés : 7-10 pour saturation MANDATORY actuelles
  Progressivité : OK (ordre tours 1-2 / 3-4 / 5-6) mais frontale sur tools
                  (current + integrations en même topic, double question)
  Problèmes identifiés :
    1. user.name jamais collecté → onboarding ne se clôture jamais côté UI
    2. Doublon user.tools.current (free text) ↔ user.tools.integrations (list)
    3. user.preferences.language inférée mais sans valeur en aval
    4. Profil operator/builder détecté mais jamais persisté (onboarding.profile_type)
    5. Pas d'aboutissement actionnable : suggested_agents jamais écrit
    6. Knowledge platform datée (manque MCP concret, governance.db, STT, A2A)

COMPAT SDK 0.3.0 :
  Signatures correctes : OUI
    - ctx.memory.remember(key, value, source=None, confidence=None)
        → crates/apollia-aip/src/memory.rs:124  (kwargs OK, behavior : skip si
          confidence existante > nouvelle)
    - ctx.memory.record(content, importance=None, task_id=None, metadata=None,
                        expires_in=None)
        → crates/apollia-aip/src/memory.rs:81
    - ctx.memory.recall(key) -> str | None
        → crates/apollia-aip/src/memory.rs:161
    - ctx.llm.complete(messages) -> LlmResponse(.content)
        → crates/apollia-aip/src/llm.rs:204
  Aucune migration de signature requise.

================================================================================
PHASE 2 — SCHÉMA CIBLE (refonte v2.1.0)
================================================================================

Identité & contexte métier (turns 1-2)  ─────────  topic : "identity"
  user.name              str           Prénom (libre)
                                        Capture : question directe (Tour 1)
                                        Lu par : desktop mandatory_fields, header UI
  user.role              enum          [operateur | builder | les-deux]
                                        Capture : question directe + inférence
                                        Lu par : desktop mode + apollia-guide profile
  user.goals             str           Objectif principal en 1 phrase
                                        Capture : question ouverte
                                        Lu par : desktop "Mon profil"
  user.domain.sector     enum          [finance | rse | legal | marketing | tech
                                        | health | hr | autre]
                                        Capture : question directe ou inférence
                                        Lu par : desktop topic + suggested_agents
  user.domain.team_size  enum          [solo | petite-equipe | entreprise]
                                        Capture : question directe
                                        Lu par : desktop ; HITL recommandation

Profil technique (turns 3-4)  ──────────────────  topic : "identity" + "tools"
  user.expertise         enum          [non-technique | dev-junior | dev-senior
                                        | ml-engineer]
                                        Capture : inféré depuis role + vocabulaire
                                                  fallback question directe
                                        Lu par : desktop topic identity
  user.tools.current     list[str]     Stack actuelle ("Notion, Gmail, Python…")
                                        Capture : question ouverte unique
                                        Lu par : desktop "Habits" view
                                        Note : remplace l'ancien free-text +
                                               nourrit aussi integrations par
                                               inférence

Objectifs d'automatisation (turns 5-6)  ────────  topic : "agents"
  user.agents.domains    list[str]     Domaines à automatiser
                                        ex. ["veille", "email-triage", "reporting"]
                                        Capture : question directe avec exemples
                                        Lu par : desktop completion gate +
                                                 suggested_agents
  user.agents.trigger    enum          [manuel | planifie | evenement]
                                        Capture : question directe vulgarisée
                                        Lu par : desktop ; recommandation /triggers
  user.agents.hitl       enum          [approbation-totale | hybride | full-auto]
                                        Capture : question directe vulgarisée
                                        Lu par : desktop ; recommandation governance

Contraintes (turns 5-6)  ───────────────────────  topic : "preferences" + "tools"
  user.preferences.sovereignty enum    [critique | important | flexible]
                                        Capture : question directe
                                        Lu par : desktop /llm reco + apollia-guide
  user.tools.integrations list[str]    MCP/services à connecter
                                        ex. ["Gmail", "Notion", "Slack"]
                                        Capture : inférée depuis tools.current
                                                  + question complémentaire
                                        Lu par : desktop /integrations reco

Préférences fines (turns 7+)  ──────────────────  topic : "preferences"
  user.preferences.llm   enum          [local | cloud | les-deux | pas-de-preference]
                                        Capture : question directe avec contexte
                                                  sovereignty
                                        Lu par : desktop /llm
  user.preferences.language str        ISO court ("fr"|"en") inférée depuis chat
                                        Capture : inférence silencieuse
                                        Lu par : desktop i18n personnalisation

État de l'onboarding  ──────────────────────────  topic : meta
  onboarding.state        json         {"started": bool, "turns": int}
                                        Écrit à chaque tour
  onboarding.active_profile enum       [operator | builder]   (lu, écrit par desktop)
  onboarding.profile_type   enum       [operator | builder]   (déduit par l'agent)
                                        Écrit dès que role connu
  onboarding.suggested_agents list[str] Démos suggérés en clôture
                                        ex. ["veille-ia", "email-triage", "veille-rse"]
                                        Lu par : desktop /agents pre-selection
  onboarding.completed      bool       Écrit en fin de conversation (résumé délivré)

Apollia features used:
  - ConversationalAgent inheritance (apollia.agents)
  - Semantic memory persistence (ctx.memory.remember / ctx.memory.recall)
  - Episodic memory (ctx.memory.record)
  - LLM-driven dialogue (ctx.llm.complete)

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

MEMORY_KEY_ONBOARDING_STATE: str = "onboarding.state"
MEMORY_KEY_ACTIVE_PROFILE: str = "onboarding.active_profile"
MEMORY_KEY_PROFILE_TYPE: str = "onboarding.profile_type"
MEMORY_KEY_SUGGESTED_AGENTS: str = "onboarding.suggested_agents"
MEMORY_KEY_COMPLETED: str = "onboarding.completed"

CONFIDENCE_EXPLICIT: float = 0.9
CONFIDENCE_INFERRED: float = 0.6
CONFIDENCE_VALIDATED: float = 0.95

# Functional keys that MUST be collected — they drive desktop completion gate
# (mandatory_fields_collected expects user.name + user.role) and demo agent
# recommendation (user.agents.domains).
MANDATORY_KEYS: tuple[str, ...] = (
    "user.name",
    "user.role",
    "user.agents.domains",
)

# Full schema — see Phase 2 header for who reads what downstream.
ONBOARDING_MEMORY_SCHEMA: dict[str, dict[str, str]] = {
    # --- Identity (desktop topic: "identity") ---
    "user.name": {"type": "string", "topic": "identity"},
    "user.role": {"type": "string", "topic": "identity"},
    "user.goals": {"type": "string", "topic": "identity"},
    "user.expertise": {"type": "string", "topic": "identity"},
    # --- Domain context (desktop topic: "domain") ---
    "user.domain.sector": {"type": "string", "topic": "domain"},
    "user.domain.team_size": {"type": "string", "topic": "domain"},
    # --- Tools & integrations (desktop topic: "tools") ---
    "user.tools.current": {"type": "list[string]", "topic": "tools"},
    "user.tools.integrations": {"type": "list[string]", "topic": "tools"},
    # --- Preferences (desktop topic: "preferences") ---
    "user.preferences.language": {"type": "string", "topic": "preferences"},
    "user.preferences.llm": {"type": "string", "topic": "preferences"},
    "user.preferences.sovereignty": {"type": "string", "topic": "preferences"},
    # --- Automation objectives (desktop topic: "agents") ---
    "user.agents.domains": {"type": "list[string]", "topic": "agents"},
    "user.agents.trigger": {"type": "string", "topic": "agents"},
    "user.agents.hitl": {"type": "string", "topic": "agents"},
}


# ---------------------------------------------------------------------------
# Topic guides
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class TopicGuide:
    """Definition of an onboarding topic the agent explores.

    Order matters: topics listed first are explored earlier in the
    conversation. Each guide tells the LLM what to collect, when to ask,
    and which memory keys to populate.
    """

    name: str
    domain: str
    objective: str
    phase: str
    memory_keys: tuple[str, ...]
    example_questions: tuple[str, ...]
    adaptation_rules: tuple[str, ...]


TOPIC_IDENTITY = TopicGuide(
    name="identity",
    domain="Identity, role and primary goal",
    objective=(
        "Capture the user's first name, their role (operator vs builder), and the "
        "main objective that brought them to Apollia. These three drive every "
        "subsequent question and the desktop completion gate."
    ),
    phase="turns 1-2",
    memory_keys=(
        "user.name",
        "user.role",
        "user.goals",
    ),
    example_questions=(
        "First, how should I call you?",
        "What kind of work brings you to Apollia today — automating recurring tasks, "
        "or building your own AI agents?",
        "In one sentence, what do you want Apollia to help you with first?",
    ),
    adaptation_rules=(
        "MANDATORY: user.name AND user.role MUST be collected before turn 4. "
        "Without them the desktop UI keeps the onboarding banner blocked.",
        "Operator signals: 'automate', 'no code', 'recurring', 'no time', 'busy', "
        "non-technical job title → role=operateur.",
        "Builder signals: 'build agents', 'Python', 'SDK', 'LangGraph', 'CrewAI', "
        "'pipeline', 'MCP server' → role=builder.",
        "Both signals together → role=les-deux.",
        "user.goals MUST be a single sentence in the user's own words — never invent.",
    ),
)

TOPIC_DOMAIN = TopicGuide(
    name="domain",
    domain="Professional context",
    objective=(
        "Identify the user's industry sector and team size. These drive demo "
        "agent selection (RSE → veille-rse, finance → reporting…) and HITL "
        "recommendations (entreprise → tighter governance)."
    ),
    phase="turns 1-2",
    memory_keys=(
        "user.domain.sector",
        "user.domain.team_size",
    ),
    example_questions=(
        "Which field do you work in? (finance, sustainability/RSE, legal, marketing, "
        "tech, healthcare, HR, other)",
        "Are you working solo, in a small team, or inside a larger organisation?",
    ),
    adaptation_rules=(
        "If the user mentions their company, role, or industry spontaneously, INFER "
        "the sector instead of asking a redundant question.",
        "Sector → demo mapping: rse → veille-rse; finance → reporting + veille-ia; "
        "legal → contract-review; marketing → veille-ia + content; tech → "
        "ci-watch + veille-ia; default → veille-ia + email-triage.",
        "team_size: solo → recommend manual/scheduled triggers and full-auto; "
        "petite-equipe → hybride HITL; entreprise → mention governance.db and "
        "approval scopes.",
    ),
)

TOPIC_TECH = TopicGuide(
    name="tools",
    domain="Technical profile and existing stack",
    objective=(
        "Determine the user's technical expertise and capture the tools they "
        "already use daily. The stack feeds MCP integration recommendations on "
        "the /integrations page; expertise drives vocabulary."
    ),
    phase="turns 3-4",
    memory_keys=(
        "user.expertise",
        "user.tools.current",
    ),
    example_questions=(
        "Which tools sit at the centre of your daily work? (Gmail, Notion, Slack, "
        "Excel, VS Code, Python, …)",
        "On a scale from non-technical to ML engineer, where would you put yourself?",
    ),
    adaptation_rules=(
        "user.expertise should usually be INFERRED from role + vocabulary, not asked: "
        "operateur + non-dev tools → non-technique; builder + Python only → "
        "dev-junior or dev-senior depending on framework mentions; LangGraph/CrewAI/"
        "PyTorch/HuggingFace → ml-engineer.",
        "Only ask the expertise question if signals are ambiguous after turn 3.",
        "user.tools.current is a comma-joined list of normalised service names — "
        "examples: 'Gmail, Notion, Slack' (NOT 'I use Gmail every day').",
        "If the user lists tools that map to known MCP servers (Gmail, Notion, "
        "Slack, GitHub, Linear, Jira), pre-fill user.tools.integrations with the "
        "matching list via [INFER user.tools.integrations=...].",
    ),
)

TOPIC_AUTOMATION = TopicGuide(
    name="agents",
    domain="Automation objectives and supervision",
    objective=(
        "Identify the workflows the user wants to automate first, the trigger "
        "mode they prefer, and the level of autonomy they accept. This is the "
        "core outcome — the demo agents on /agents are pre-selected from "
        "user.agents.domains."
    ),
    phase="turns 5-6",
    memory_keys=(
        "user.agents.domains",
        "user.agents.trigger",
        "user.agents.hitl",
    ),
    example_questions=(
        "Which recurring task would you like Apollia to handle first? Common "
        "starting points: automated tech watch, email triage, RSE/ESG reporting, "
        "meeting summaries, customer-support triage, daily news brief.",
        "Should agents run on a schedule (e.g. every morning at 9h), react to "
        "events (new email, file change, webhook), or only when you click run?",
        "Do you want to approve every agent action before it happens, only review "
        "the important decisions, or let agents run fully on their own?",
    ),
    adaptation_rules=(
        "MANDATORY: user.agents.domains MUST be collected. If the user is unsure, "
        "PROPOSE 2 concrete options grounded in their sector and stack.",
        "agents.domains is a list of slugs: veille | email-triage | reporting | "
        "meeting-summary | customer-support | content | contract-review | "
        "ci-watch (free additions allowed but slug-shaped — lowercase-hyphen).",
        "agents.trigger values: manuel | planifie | evenement.",
        "agents.hitl values: approbation-totale | hybride | full-auto.",
        "Operator vocabulary: 'click OK', 'approve', 'review' instead of HITL/step "
        "budget/governance scope.",
        "Builder vocabulary: HITL steps, step budget, approval scopes "
        "(session/project/global), governance.db.",
    ),
)

TOPIC_PREFERENCES = TopicGuide(
    name="preferences",
    domain="LLM, sovereignty and integrations",
    objective=(
        "Capture data sovereignty requirements, LLM backend preference, and any "
        "integrations the user wants beyond the tools they already use. The "
        "sovereignty answer determines which LLM backends are recommended on /llm."
    ),
    phase="turns 7+",
    memory_keys=(
        "user.preferences.sovereignty",
        "user.preferences.llm",
        "user.preferences.language",
        "user.tools.integrations",
    ),
    example_questions=(
        "How important is it that your data never leaves your machine — critical "
        "(local-first, offline LLM only), important (local by default, cloud OK "
        "with consent), or flexible?",
        "Do you prefer a fully local LLM (llama.cpp, Ollama, no API key needed) "
        "or a cloud model (Claude, GPT-4, Bedrock, Vertex)?",
        "Beyond the tools you already use, are there services you'd like Apollia "
        "to connect to via MCP — Gmail, Notion, Slack, GitHub, Linear?",
    ),
    adaptation_rules=(
        "INFER user.preferences.language from the language the user writes in — "
        "store ISO short code 'fr' or 'en'. Never ask.",
        "Sovereignty mapping: 'local-first', 'RGPD', 'no cloud', 'private', "
        "'offline' → critique. 'OK if encrypted/EU' → important. 'whatever works' "
        "→ flexible.",
        "If sovereignty=critique and user mentions 'Claude'/'OpenAI', GENTLY "
        "highlight the conflict and suggest llama.cpp (Llama 3, Qwen, Mistral) as "
        "default with cloud opt-in.",
        "If undecided about LLM → llm=pas-de-preference and recommend trying local "
        "first (zero cost, zero key).",
        "user.tools.integrations augments the values already inferred from "
        "user.tools.current — do not duplicate.",
    ),
)

TOPIC_GUIDES: dict[str, TopicGuide] = {
    "identity": TOPIC_IDENTITY,
    "domain": TOPIC_DOMAIN,
    "tools": TOPIC_TECH,
    "agents": TOPIC_AUTOMATION,
    "preferences": TOPIC_PREFERENCES,
}

ALL_TOPIC_MEMORY_KEYS: frozenset[str] = frozenset(
    key
    for guide in TOPIC_GUIDES.values()
    for key in guide.memory_keys
)


def topic_for_memory_key(key: str) -> str | None:
    """Return the topic name a memory key belongs to, or ``None``."""
    for topic_name, guide in TOPIC_GUIDES.items():
        for mk in guide.memory_keys:
            if key == mk or key.startswith(mk + "."):
                return topic_name
    return None


def _build_topic_section(guide: TopicGuide) -> str:
    """Render a single topic guide as a system prompt section."""
    lines = [
        f"### {guide.domain}  [{guide.phase}]",
        f"Objective: {guide.objective}",
        f"Memory keys: {', '.join(guide.memory_keys)}",
        "Example questions:",
    ]
    for q in guide.example_questions:
        lines.append(f'  - "{q}"')
    lines.append("Adaptation:")
    for r in guide.adaptation_rules:
        lines.append(f"  - {r}")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------

_TOPIC_SECTIONS = "\n\n".join(
    _build_topic_section(g) for g in TOPIC_GUIDES.values()
)

_SYSTEM_PROMPT = f"""\
You are the onboarding assistant for Apollia OS — a local-first Rust runtime \
for autonomous AI agents. Apollia lets professionals run agents that perceive, \
reason, and act on their behalf, entirely on their own machine, without any \
mandatory cloud dependency.

## Apollia capabilities (mention naturally, never invent)

- **Agents**: ReAct autonomous agents, ConversationalAgent, OrchestratedAgent \
networks (A2A protocol), all written as plain Python (`manifest()` + async \
`run()`).
- **Triggers**: cron, interval, file_watch, webhook, oneshot — hot-reloaded \
from SQLite.
- **Pipelines**: agent DAGs with fan-out/fan-in and human-in-the-loop (HITL) \
approval steps.
- **Memory**: episodic, semantic, procedural — full export/import, agent-scoped \
namespaces.
- **Native tools**: web_search, web_read, http_fetch, bash, file_read, \
file_write, file_list, email_send (13 native tools, sandboxed and audited).
- **Tool governance**: human approval with scopes session / project / global, \
unified `governance.db`, audit trail.
- **STT**: voice dictation and transcription via whisper-rs (fully local).
- **LLM backends**: Anthropic Claude, OpenAI, Ollama, llama.cpp (Metal/CUDA, \
GGUF), Bedrock, Vertex — selected via the `/llm` page; LlmRouter picks per \
agent.
- **MCP integrations**: native client (stdio / HTTP / SSE) — Gmail, Notion, \
Slack, GitHub, and any MCP-compatible server.

## Progressive disclosure — FOLLOW THIS ORDER, NEVER JUMP AHEAD

Trust builds gradually. Capture light, business-level facts first; capture \
sensitive or precise preferences only once rapport is established.

  Turns 1-2 : identity (name, role, goals) + domain (sector, team size)
  Turns 3-4 : tech profile (expertise, current tools) — infer when you can
  Turns 5-6 : automation (domains to automate, trigger, supervision level)
  Turns 7+  : constraints & fine preferences (sovereignty, LLM, integrations),
              then conclude.

## Topics to explore

{_TOPIC_SECTIONS}

## Memory rules

- When the user states a durable fact explicitly, emit: `[REMEMBER key=value]`.
- When you deduce a durable fact from context (vocabulary, sector, language), \
emit: `[INFER key=value]`.
- Use the EXACT keys from the schema above (e.g. `user.role`, \
`user.agents.domains`). Never invent new keys.
- For list-typed keys (`user.agents.domains`, `user.tools.current`, \
`user.tools.integrations`), use a comma-joined slug list. Example: \
`[REMEMBER user.agents.domains=veille,email-triage]`.
- Emit tags at the END of your message — never inline with the visible text.
- Only memorise durable facts (still true in 6 months). Never memorise current \
tasks, mood, or transient details.
- Each [REMEMBER] must enable a concrete downstream action. Ask: which page or \
agent will use this value? If none → skip it.
- As soon as user.role is known, emit \
`[REMEMBER onboarding.profile_type=operator]` or `=builder` (use `builder` if \
role is `builder` or `les-deux`, else `operator`).

## Conversation rules

- Open with a warm 2-sentence introduction of Apollia OS, then ask the user's \
first name AND what they want to automate or build (one combined turn).
- Ask ONE question at a time after that. Build on the previous answer.
- Never ask a numbered list of questions.
- Detect the profile (operator / builder) by turn 2 and adapt vocabulary \
immediately.
- When user.name, user.role, AND user.agents.domains are all collected, AND \
at least one key is populated for each of the 5 topics, conclude with: a 3-4 \
sentence personalised summary; a `[REMEMBER onboarding.completed=true]` tag; \
a `[REMEMBER onboarding.suggested_agents=...]` tag listing 2-3 demo agent \
slugs picked from the user's sector + automation domains; and 1-2 concrete \
next steps (e.g. "Open /agents — I pre-selected veille-ia and email-triage \
for you" or "Head to /llm to wire up your local Llama model").
- Always respond in the same language as the user's message.\
"""


# ---------------------------------------------------------------------------
# Profile-specific system prompt sections
# ---------------------------------------------------------------------------

OPERATOR_SECTION = """\

## Operator profile detected

You are talking to an Operator — someone who wants to automate recurring tasks \
without writing code. Adapt every word:
- Say "task" not "agent run", "automate" not "deploy", "approve" not "HITL", \
"connect" not "MCP integration".
- Never mention SDK, manifest, async/await, DAG, step budget, A2A.
- Frame demos in terms of outcomes ("get a daily news brief in your inbox") \
not mechanics ("a ReAct agent calling web_search every morning").
Focus the remaining turns on:
- Which repetitive task they'd eliminate first (email triage, weekly RSE \
report, automated tech watch, meeting summaries…).
- Whether they prefer to click-approve each action or only key decisions.
- Which trigger fits — scheduled (every morning at 9h), event-based (new email \
arrives, file dropped in folder), or manual (click run).\
"""

BUILDER_SECTION = """\

## Builder profile detected

You are talking to a Builder — a developer who wants to create their own \
agents, pipelines, triggers, and MCP integrations. You may use full technical \
vocabulary:
ConversationalAgent, OrchestratedAgent, manifest(), async run(), StepBudget, \
HITL steps, governance scopes, MCP stdio/HTTP/SSE, A2A protocol, fan-out/fan-in, \
trigger engine (cron/interval/file_watch/webhook/oneshot).
Focus the remaining turns on:
- Python/async comfort and any agent framework already used (LangGraph, \
CrewAI, AutoGen, custom).
- Whether they want to build standalone agents, pipelines, MCP servers, or \
all of the above.
- Preferred LLM backend (local llama.cpp/Ollama vs cloud Anthropic/OpenAI/\
Bedrock/Vertex) and routing strategy.\
"""


def build_system_prompt_with_profile(profile: str | None) -> str:
    """Build the full system prompt with an optional profile-specific section."""
    if profile == "operator":
        return _SYSTEM_PROMPT + OPERATOR_SECTION
    if profile == "builder":
        return _SYSTEM_PROMPT + BUILDER_SECTION
    return _SYSTEM_PROMPT


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

_REMEMBER_RE = re.compile(r"\[REMEMBER\s+([^\]=]+)=([^\]]+)\]")
_INFER_RE = re.compile(r"\[INFER\s+([^\]=]+)=([^\]]+)\]")
_META_KEY_PREFIXES: tuple[str, ...] = ("onboarding.",)


def _normalise_key(raw_key: str) -> str:
    """Lower-case, replace spaces with underscores, ensure namespace prefix.

    Keys belonging to the ``onboarding.*`` meta namespace are preserved as-is.
    Any other key is forced into the ``user.`` namespace.
    """
    key = raw_key.strip().lower().replace(" ", "_")
    if any(key.startswith(p) for p in _META_KEY_PREFIXES):
        return key
    if not key.startswith(MEMORY_KEY_PREFIX):
        key = MEMORY_KEY_PREFIX + key
    return key


def _extract_remember_tags(text: str) -> list[tuple[str, str]]:
    """Extract ``[REMEMBER key=value]`` pairs from LLM output."""
    return [
        (_normalise_key(m.group(1)), m.group(2).strip())
        for m in _REMEMBER_RE.finditer(text)
    ]


def _extract_infer_tags(text: str) -> list[tuple[str, str]]:
    """Extract ``[INFER key=value]`` pairs from LLM output."""
    return [
        (_normalise_key(m.group(1)), m.group(2).strip())
        for m in _INFER_RE.finditer(text)
    ]


def _strip_remember_tags(text: str) -> str:
    """Remove ``[REMEMBER ...]`` and ``[INFER ...]`` tags from user-facing text."""
    cleaned = _REMEMBER_RE.sub(" ", text)
    cleaned = _INFER_RE.sub(" ", cleaned)
    return re.sub(r"\s+", " ", cleaned).strip()


async def persist_insight(
    ctx: Any,
    key: str,
    value: str,
    explicit: bool = True,
) -> None:
    """Persist an onboarding insight with the appropriate confidence score.

    Explicit information (stated directly) gets ``CONFIDENCE_EXPLICIT`` (0.9).
    Inferred information (deduced from context) gets ``CONFIDENCE_INFERRED`` (0.6).
    The memory layer skips writes when the existing entry holds strictly
    higher confidence.
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

    Guides new users through a progressive dialogue covering identity, domain,
    technical profile, automation objectives, and constraints. Each durable
    fact is persisted immediately via semantic memory.
    """

    SYSTEM_PROMPT = _SYSTEM_PROMPT
    MAX_TURNS = 30
    TEMPERATURE = 0.7

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest."""
        return {
            "name": "onboarding-agent",
            "version": "2.1.0",
            "description": (
                "Conversational onboarding agent — captures identity, role, "
                "automation goals, and constraints through progressive dialogue."
            ),
            "execution_mode": "auto",
            "agent_type": "system",
            "tools_required": [],
            "tools_optional": [],
            "memory_namespace": "onboarding-agent",
            "max_concurrent_tasks": 1,
            "dangerous_tools_allowed": False,
            "tags": ["onboarding", "conversational"],
        }

    def on_response(self, response: str) -> str:
        """Strip internal REMEMBER/INFER tags before showing to the user."""
        return _strip_remember_tags(response)

    async def converse(
        self,
        ctx: Any,
        user_message: str,
        history: list[dict[str, str]] | None = None,
    ) -> tuple[str, list[dict[str, str]]]:
        """Drive one turn of profile-aware conversation with memory extraction."""
        is_first_message = not history

        if is_first_message:
            active_profile: str | None = None
            if ctx.memory is not None:
                try:
                    active_profile = await ctx.memory.recall(MEMORY_KEY_ACTIVE_PROFILE)
                except Exception:
                    active_profile = None

            self.SYSTEM_PROMPT = build_system_prompt_with_profile(active_profile)

        if ctx.llm is None:
            raise RuntimeError(
                "OnboardingAgent requires ctx.llm — no LLM backend configured"
            )

        messages: list[dict[str, str]] = list(history) if history else []

        if not messages or messages[0].get("role") != "system":
            messages.insert(0, {"role": "system", "content": self.SYSTEM_PROMPT})

        messages.append({"role": "user", "content": user_message})

        response = await ctx.llm.complete(messages)
        # LlmResponse is a PyO3 object exposing .content (see apollia-aip/src/llm.rs).
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
                key=MEMORY_KEY_ONBOARDING_STATE,
                value=json.dumps({"started": True, "turns": 1}),
                source=MEMORY_SOURCE,
                confidence=CONFIDENCE_EXPLICIT,
            )

        return processed_text, messages

    async def run(self, task: Any, ctx: Any) -> AIPResult:
        """Execute one onboarding turn from an AIPTask payload."""
        if ctx.llm is None:
            raise RuntimeError(
                "OnboardingAgent requires ctx.llm — no LLM backend configured"
            )

        # task is a dict (serialised from Rust AIPTask via JSON).
        # task["input"]["parts"][0]["text"] holds the user message.
        task_input = task.get("input") if isinstance(task, dict) else getattr(task, "input", None)
        if task_input is None:
            return AIPResult.failed("NO_INPUT", "No input provided in task")

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

        # task["history"] is a list of {"role": "user"|"agent", "parts": [{"text": ...}]}.
        raw_history = task.get("history", []) if isinstance(task, dict) else getattr(task, "history", [])
        history = []
        for msg in (raw_history or []):
            if isinstance(msg, dict):
                role_raw = msg.get("role", "user")
                role = "assistant" if role_raw == "agent" else role_raw
                parts = msg.get("parts", [])
                text = parts[0]["text"] if parts and isinstance(parts[0], dict) else str(msg)
                history.append({"role": role, "content": text})
            elif hasattr(msg, "role"):
                role = "assistant" if msg.role == "agent" else msg.role
                parts = getattr(msg, "parts", [])
                text = parts[0].text if parts else str(msg)
                history.append({"role": role, "content": text})

        response_text, _ = await self.converse(ctx, input_text, history=history or None)
        return AIPResult.completed(response_text)


# ---------------------------------------------------------------------------
# Module-level agent instance (required by the Apollia AIP contract)
# ---------------------------------------------------------------------------

agent = OnboardingAgent()

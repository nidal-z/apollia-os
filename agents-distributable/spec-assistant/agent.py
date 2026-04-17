"""spec-assistant — Consultant agent for feature specification on Apollia OS.

Premier maillon du pipeline de développement (spec → dev → review). Transforme
toute idée ou demande floue en TaskSpec structurée, sauvegardée dans le
workspace du projet sous ``.apollia/tasks/{slug}.md``.

Fonctionnement
--------------
Agent ReAct agnostique. Le LLM décide au fil de la conversation quels outils
appeler — ``ask_user`` pour qualifier le besoin, ``file_read`` pour lire le
contexte existant, ``file_write`` pour sauver la TaskSpec, ``memory_search``
pour détecter les doublons. Aucune regex spécifique à une stack ni de
post-processing déterministe : l'agent s'adapte au contexte comme un
consultant qui découvre un projet.

Contexte pré-chargé
-------------------
Deux hooks inline à chaque tour : ``workspace_rules(ctx)`` (lu depuis
``ctx.workspace.rules``, zero persistence) et ``discover_task_specs(ctx)``
(un ``ls`` pour lister les specs existantes). Injectés dans le system
prompt.

Outils requis : file_read, file_write, ask_user
Outils optionnels : bash_executor, file_list, memory_search
"""

from __future__ import annotations

import re
import unicodedata
from typing import Any

from apollia.agents import AIPResult, BaseReActAgent
from apollia.utils.hitl import resume_pending_tool

from lib import discover_task_specs, workspace_rules


# ---------------------------------------------------------------------------
# Slug helper (exposed so the LLM / tests can suggest filenames consistently)
# ---------------------------------------------------------------------------

_SLUG_NON_ALNUM: re.Pattern[str] = re.compile(r"[^a-z0-9]+")


def slugify(title: str) -> str:
    """Convert *title* to a URL-safe lowercase slug (max 64 chars).

    Deterministic helper kept for the LLM's convenience (the system prompt
    tells it to derive a slug from the title). No behavioural decision is
    tied to this function.
    """
    normalized = unicodedata.normalize("NFD", title.lower())
    ascii_str = normalized.encode("ascii", "ignore").decode("ascii")
    slug = _SLUG_NON_ALNUM.sub("-", ascii_str).strip("-")
    return (slug or "spec")[:64]


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------

_SYSTEM_PROMPT_TEMPLATE: str = """\
You are **spec-assistant**, a senior IT consultant specialised in feature \
design and scope qualification.

Your job is to turn any idea, request, or fuzzy need into a structured \
**TaskSpec** — a document a third party can pick up and execute without \
guessing. You work like a consultant from a development agency: you never \
invent the user's context, you ask.

## Core behaviour

**Qualify before specifying.** Before writing any spec, make sure you \
understand:
- What problem the user is solving, who the end users are, what success looks \
  like
- The stack / platform / domain constraints, if any
- The scope boundaries — what's in, what's explicitly out
- Existing conventions (project rules, similar specs already created)

When you lack context, call **ask_user** with batched, structured questions \
(1 call, multiple questions). **Adapt the questions to the domain** — a \
marketing website requires different qualification than a data pipeline or \
a mobile app. Never use a generic question template — formulate questions \
that match what the user just described.

**Specify the what, not the how.** The TaskSpec describes outcomes, not \
implementations. "Users can export their data to CSV in one click" — not \
"add a GET /export endpoint with ?format=csv".

**Never invent.** If a detail isn't in the user's answers or the project \
rules, ask — don't fabricate requirements, organisation, processes, roles.

**Adapt formalism to context.** A simple UI tweak doesn't need the same \
decomposition as a database migration. Adjust depth, sections, and \
acceptance criteria to what's actually useful.

**Every acceptance criterion must be verifiable.** "Works correctly" is not \
a criterion. "The form shows an error message when the email is invalid" is.

## Workflow

1. **Read the context below.** If project rules or existing specs are loaded, \
   leverage them. If not, you'll need to qualify the request from scratch.
2. **Qualify via ask_user** when the request lacks critical context. Batch \
   your questions — one `ask_user` call with 3-6 well-chosen questions is \
   better than several round-trips.
3. **Detect duplicates.** For new specs, call `memory_search` with keywords \
   from the request. If a near-duplicate already exists, propose refining \
   the existing one rather than creating a new one.
4. **Write the TaskSpec incrementally.**
   - Call `file_write` **once** with a minimal skeleton (file header + \
     section titles only, no body content yet). Keep the content string \
     short — 200-400 chars max. This avoids JSON-escaping issues with \
     long multi-line markdown.
   - Then call `file_edit` **one section at a time** to fill each section \
     body. `file_edit` replaces a snippet in the file — pick a unique \
     anchor (e.g. the section title line) as `old_text` and provide the \
     filled-in section as `new_text`.
   - Derive a short URL-safe slug from the title (lowercase, hyphens, no \
     accents, max 64 chars). Save at `.apollia/tasks/{{slug}}.md`.
5. **Confirm** with a `final_answer` that summarises what was saved and \
   what the user should review.

## JSON escaping — CRITICAL

When you emit `file_write` or `file_edit` with multi-line markdown in \
`content`/`new_text`/`old_text`, you must emit **valid JSON**:

- Newlines inside the string are `\\n` (backslash + n), never a literal \
  line break.
- Double quotes inside the string are `\\"` (backslash + quote), never a \
  bare `"`. Markdown titles like `"Title"` must be emitted as \
  `\\"Title\\"` inside the JSON.
- Backslashes themselves are `\\\\` (two backslashes).

If you cannot confidently emit a long string with all quotes escaped, use \
the incremental workflow above (skeleton + file_edit per section) — each \
section body stays short enough to escape reliably.

## TaskSpec structure (flexible — adapt to context)

At minimum a TaskSpec contains:
- **Objective** — user-visible outcome
- **Layers impacted** — parts of the system that change (frontend, backend, \
  data, ops, content, process…)
- **Scope in** — what's included
- **Scope out** — what's explicitly excluded, to prevent scope creep
- **Acceptance criteria** — verifiable conditions marking "done"

Optional sections when they add real information: assumptions, risks, \
dependencies, open questions. Never pad with generic filler.

## What you never do

- Never generate code, snippets, or commands — you write specs, not code.
- Never write a TaskSpec without qualifying the request when context is \
  missing.
- Never invent organisational context (team size, roles, processes) unless \
  explicitly told.

## Project context

{rules_section}

## Existing specs

{specs_section}

## Language

Always respond in the same language as the user's message. Detect it from \
the input and mirror it naturally.
"""


def _build_system_prompt(
    raw_rules: str,
    existing_specs: list[str] | None,
) -> str:
    """Compose the system prompt, injecting workspace rules and known specs."""
    rules = (raw_rules or "").strip()
    if rules:
        truncated = rules[:4000]
        if len(rules) > 4000:
            truncated += "\n[... truncated for context window ...]"
        rules_section = (
            "**Workspace rules loaded** (project-specific conventions — "
            "treat these as authoritative for this project):\n"
            f"```\n{truncated}\n```"
        )
    else:
        rules_section = (
            "No rules file found in this workspace. "
            "Ask the user about their conventions and constraints via "
            "`ask_user` before writing the spec."
        )

    if existing_specs:
        slugs = "\n".join(f"- `{s}`" for s in sorted(existing_specs))
        specs_section = (
            f"The following TaskSpecs already exist in `.apollia/tasks/`:\n"
            f"{slugs}\n\n"
            "Consider refining one of these if the new request is similar — "
            "read it first with `file_read`."
        )
    else:
        specs_section = (
            "No existing specs in this project — this will be the first."
        )

    return _SYSTEM_PROMPT_TEMPLATE.format(
        rules_section=rules_section, specs_section=specs_section
    )


# ---------------------------------------------------------------------------
# Task input extraction
# ---------------------------------------------------------------------------

def _extract_task_input(task: Any) -> tuple[str, list[dict[str, str]]]:
    """Extract the user message and conversation history from *task*.

    Supports dict format (API / resume_pending_tool path), objects with
    attributes (runtime PyO3 format), and A2A multi-turn parts.
    History entries are normalised to
    ``{"role": "user"|"assistant", "content": "..."}``.
    """
    task_input = (
        task.get("input") if isinstance(task, dict)
        else getattr(task, "input", None)
    )
    if task_input is None:
        return "", []

    if isinstance(task_input, dict):
        parts = task_input.get("parts", [])
        input_text: str = (
            parts[0]["text"]
            if parts and isinstance(parts[0], dict)
            else str(task_input.get("text", ""))
        )
    elif hasattr(task_input, "parts"):
        parts = task_input.parts
        input_text = parts[0].text if parts else str(task_input)
    elif hasattr(task_input, "text"):
        input_text = task_input.text
    else:
        input_text = str(task_input)

    raw_history = (
        task.get("history", []) if isinstance(task, dict)
        else getattr(task, "history", [])
    )
    history: list[dict[str, str]] = []
    for msg in raw_history or []:
        if isinstance(msg, dict):
            role_raw = msg.get("role", "user")
            role = "assistant" if role_raw == "agent" else role_raw
            parts = msg.get("parts", [])
            text = (
                parts[0]["text"]
                if parts and isinstance(parts[0], dict)
                else str(msg)
            )
            history.append({"role": role, "content": text})
        elif hasattr(msg, "role"):
            role = "assistant" if msg.role == "agent" else msg.role
            parts = getattr(msg, "parts", [])
            text = parts[0].text if parts else str(msg)
            history.append({"role": role, "content": text})

    return input_text, history


# ---------------------------------------------------------------------------
# Manifest
# ---------------------------------------------------------------------------

def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for spec-assistant."""
    return {
        "name": "spec-assistant",
        "version": "2.0.0",
        "description": (
            "Consultant agent that turns any idea or fuzzy need into a "
            "structured, actionable TaskSpec saved to `.apollia/tasks/`. "
            "Works like a senior IT consultant: qualifies the need via "
            "batched questions, adapts to any stack or domain, never "
            "invents requirements. Never generates code — first link of the "
            "spec → dev → review pipeline."
        ),
        "execution_mode": "auto",
        "agent_type": "assistant",
        "tools_required": ["file_read", "file_write", "ask_user"],
        "tools_optional": ["bash_executor", "file_list", "memory_search"],
        "tools_requiring_approval": [],
        "packages": [],
        "memory_namespace": "spec-assistant",
        "llm_backend": "precise",
        "supports_streaming": True,
        "supports_a2a": True,
        "step_budget": {
            "max_steps": 30,
            "max_tool_calls": 20,
            "wall_clock_secs": 600,
        },
        "tags": [
            "conception", "specification", "pipeline-dev",
            "taskspec", "no-code", "consultant", "agnostic",
        ],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
        "examples": [
            "Crée une spec pour un système d'authentification JWT avec refresh tokens",
            "Crée une spec pour le site vitrine d'Apollia OS",
            "Affine la spec user-auth pour ajouter la gestion des rôles",
            "Crée une spec pour l'export CSV de la table Commandes",
            "Y a-t-il déjà une spec similaire avant d'en créer une nouvelle ?",
        ],
        "limitations": [
            "Ne génère jamais de code — uniquement des specs structurées",
            "Ne modifie aucun fichier source du projet",
            "Requiert une interaction utilisateur (via ask_user) quand le "
            "contexte initial est insuffisant",
        ],
        "setup_notes": (
            "Fonctionne sans fichier de règles projet — dans ce cas l'agent "
            "qualifie le besoin via ask_user au premier tour. Si "
            "APOLLIA.md existe dans le workspace, ses règles sont injectées "
            "dans le system prompt. Les specs précédentes sont détectées et "
            "listées automatiquement. Stockage des specs dans "
            "`.apollia/tasks/{slug}.md`, créé à la volée par `file_write`."
        ),
        "skills": [
            {
                "id": "create-spec",
                "name": "Créer une TaskSpec",
                "description": (
                    "Qualifie le besoin via des questions adaptées au "
                    "contexte, détecte les doublons éventuels, puis "
                    "sauvegarde une TaskSpec structurée dans "
                    "`.apollia/tasks/{slug}.md`."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "request": {
                        "type": "string",
                        "description": "Description de la feature ou tâche à spécifier",
                        "required": True,
                    },
                },
            },
            {
                "id": "refine-spec",
                "name": "Affiner une TaskSpec existante",
                "description": (
                    "Lit une TaskSpec existante, la révise selon les "
                    "retours utilisateur ou d'un agent en aval, réécrit le "
                    "fichier avec les ajustements."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "slug": {
                        "type": "string",
                        "description": "Slug de la TaskSpec à affiner (ex. : user-auth)",
                        "required": True,
                    },
                    "feedback": {
                        "type": "string",
                        "description": "Ce qui doit être ajusté, complété ou corrigé",
                        "required": True,
                    },
                },
            },
            {
                "id": "list-specs",
                "name": "Lister les TaskSpecs du projet",
                "description": (
                    "Retourne la liste des TaskSpecs créées dans "
                    "`.apollia/tasks/` via un simple `bash_executor`."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {},
            },
        ],
    }


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------

class SpecAssistant(BaseReActAgent):
    """Consultant agent for feature specification — first link of spec→dev→review.

    Behaviour :
    - Lit le snapshot cross-session (règles projet + specs existantes) puis
      construit un system prompt dynamique.
    - Délègue au LLM toute décision : qualifier via ``ask_user``, détecter
      les doublons via ``memory_search``, écrire la spec via ``file_write``.
    - Préserve l'historique conversationnel entre les messages utilisateur
      (mode chat multi-tour).
    - HITL natif via la boucle ReAct du SDK.
    """

    SYSTEM_PROMPT: str = _SYSTEM_PROMPT_TEMPLATE.format(
        rules_section="(loaded per-turn)",
        specs_section="(loaded per-turn)",
    )
    MAX_STEPS: int = 20
    TEMPERATURE: float = 0.3

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for spec-assistant."""
        return manifest()

    async def run(self, task: Any, ctx: Any) -> dict[str, Any]:
        """Execute one conversational turn via the ReAct loop.

        Loads workspace rules + existing spec slugs inline (no snapshot),
        builds the system prompt, then runs the ReAct loop with the
        conversational history so the LLM sees previous turns.
        """
        if ctx.llm is None:
            return AIPResult.failed(
                "NO_LLM",
                "SpecAssistant requires ctx.llm — no LLM backend configured",
            )

        input_text, history = _extract_task_input(task)
        if not input_text:
            return AIPResult.failed("NO_INPUT", "No input provided in task")

        raw_rules = workspace_rules(ctx)
        existing_specs = await discover_task_specs(ctx)

        self.SYSTEM_PROMPT = _build_system_prompt(raw_rules, existing_specs)

        pending = resume_pending_tool(task)

        result = await self.react(
            task,
            ctx,
            input_text,
            pending_tool=pending,
            history=history or None,
        )
        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)


# ---------------------------------------------------------------------------
# Module-level agent instance (AIP contract)
# ---------------------------------------------------------------------------

agent = SpecAssistant()

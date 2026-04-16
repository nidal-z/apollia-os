"""spec-assistant — Assistant de conception pour Apollia OS.

Premier maillon du pipeline de développement (spec → dev → review). Transforme
toute idée ou demande en TaskSpec structurée, sauvegardée dans le workspace du
projet sous ``.apollia/tasks/{slug}.md``.

Fonctionnement :
- Utilise ``SpecContextBootstrap`` pour charger le contexte projet (workspace
  rules, tech stack, specs existantes) et le persister en mémoire sémantique.
  En session N+1, le snapshot est rechargé sans relire les fichiers.
- Utilise ``memory.search()`` pour détecter les specs similaires existantes et
  prévenir les doublons.
- Enregistre chaque spec créée dans ``created_specs`` pour la traçabilité.
- Les réponses LLM contenant un bloc ``[SPEC:slug]…[/SPEC]`` déclenchent une
  écriture automatique dans le workspace avant retour à l'utilisateur.

Outils requis  : file_read, file_write
Outils optionnels : bash_executor (mkdir, ls), file_list (découverte workspace)
Backend LLM    : precise (qualité de spec > vitesse)
"""

from __future__ import annotations

import json
import re
import unicodedata
from typing import Any

from apollia.agents import AIPResult, ConversationalAgent

try:
    from shared.project_bootstrap import ProjectContextBootstrap
except ModuleNotFoundError:
    from assistants.shared.project_bootstrap import ProjectContextBootstrap


# ---------------------------------------------------------------------------
# Memory keys
# ---------------------------------------------------------------------------

MEMORY_KEY_CREATED_SPECS: str = "created_specs"

_MEMORY_SOURCE: str = "spec-assistant"
_MEMORY_CONFIDENCE_SPECS: float = 1.0

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# [SPEC:slug]…[/SPEC] — the LLM uses this marker to emit a TaskSpec for saving.
_SPEC_BLOCK_RE: re.Pattern[str] = re.compile(
    r"\[SPEC:([a-z0-9][a-z0-9\-]{0,62})\](.*?)\[/SPEC\]",
    re.DOTALL,
)

_SLUG_NON_ALNUM: re.Pattern[str] = re.compile(r"[^a-z0-9]+")



# ---------------------------------------------------------------------------
# Context Bootstrap
# ---------------------------------------------------------------------------


class SpecContextBootstrap(ProjectContextBootstrap):
    """Bootstrap for spec-assistant.

    Extends the common project snapshot with:

    - ``existing_specs``: slugs of TaskSpec files in ``.apollia/tasks/``
    - ``spec_count``: number of specs (used in UX messages)
    """

    async def extra_scopes(
        self,
        ctx: Any,
        base_snapshot: dict[str, Any],
    ) -> dict[str, Any]:
        """Discover existing TaskSpec files in ``.apollia/tasks/``."""
        specs: list[str] = []
        if ctx.tools is not None:
            result = await ctx.tools.call("bash_executor", {
                "command": "ls .apollia/tasks/*.md 2>/dev/null | head -50",
            })
            if result and result.get("stdout"):
                raw_stdout = result["stdout"]
                if isinstance(raw_stdout, str):
                    for line in raw_stdout.split("\n"):
                        name = line.strip()
                        if name.endswith(".md"):
                            slug = name.rsplit("/", 1)[-1][:-3]
                            if slug:
                                specs.append(slug)
        return {"existing_specs": specs, "spec_count": len(specs)}


# ---------------------------------------------------------------------------
# Slug generation
# ---------------------------------------------------------------------------

def _slugify(title: str) -> str:
    """Convert *title* to a URL-safe lowercase slug (max 64 chars)."""
    normalized = unicodedata.normalize("NFD", title.lower())
    ascii_str = normalized.encode("ascii", "ignore").decode("ascii")
    slug = _SLUG_NON_ALNUM.sub("-", ascii_str).strip("-")
    return (slug or "spec")[:64]


# ---------------------------------------------------------------------------
# Project rules parsing
# ---------------------------------------------------------------------------

def _extract_forbidden_deps(raw: str) -> list[str]:
    """Return dependency names explicitly forbidden in *raw*.

    Handles backtick-wrapped names (`` `pkg` INTERDIT ``), plain names
    (``pkg INTERDIT``), and English variants (``forbidden: pkg``).
    The ``\\W+`` between name and keyword accepts any non-word separators
    (backticks, parentheses, quotes, spaces).
    """
    patterns = [
        r"\b([a-zA-Z][\w\-]+)\b\W+INTERDIT\b",
        r"INTERDIT[^:]*:\s*([a-zA-Z][\w\-]+)",
        r"\b([a-zA-Z][\w\-]+)\b\W+interdit\b",
        r"forbidden[^:]*:\s*([a-zA-Z][\w\-]+)",
        r"\b([a-zA-Z][\w\-]+)\b\W+is\s+forbidden\b",
        r"\b([a-zA-Z][\w\-]+)\b\W+not\s+allowed\b",
        r"\bno\s+([a-zA-Z][\w\-]+)\b",
        r"\bbann?ed?\s+([a-zA-Z][\w\-]+)\b",
    ]
    found: set[str] = set()
    for pat in patterns:
        for m in re.finditer(pat, raw):
            dep = m.group(1).strip()
            if len(dep) >= 2:
                found.add(dep)
    return sorted(found)


def _extract_section_text(raw: str, *headers: str) -> str:
    """Return the text block immediately following the first matching *header*."""
    for header in headers:
        idx = raw.find(header)
        if idx == -1:
            continue
        after = raw[idx + len(header):].lstrip("\n")
        lines: list[str] = []
        for line in after.splitlines():
            if line.startswith("#") and lines:
                break
            lines.append(line)
        block = "\n".join(lines).strip()
        if block:
            return block
    return ""


def parse_project_rules(raw_text: str) -> dict[str, str]:
    """Parse *raw_text* from workspace files and return categorised rules.

    Returns a dict with keys: ``raw`` (full text, truncated),
    ``forbidden_deps`` (JSON list), ``patterns``, ``comment_convention``.
    """
    forbidden = _extract_forbidden_deps(raw_text)
    patterns = _extract_section_text(
        raw_text,
        "## Patterns obligatoires",
        "### Patterns obligatoires",
        "## Required patterns",
        "### Required patterns",
        "## Règles d'implémentation",
        "## Implementation rules",
    )
    comment_conv = _extract_section_text(
        raw_text,
        "Convention de commentaires",
        "Comment convention",
        "## Comments",
    )
    truncated = raw_text[:4_000]
    if len(raw_text) > 4_000:
        truncated += "\n[… règles tronquées pour tenir dans le contexte …]"
    return {
        "raw": truncated,
        "forbidden_deps": json.dumps(forbidden),
        "patterns": patterns[:500],
        "comment_convention": comment_conv[:200],
    }


# ---------------------------------------------------------------------------
# Created-specs tracking
# ---------------------------------------------------------------------------

async def load_created_specs(ctx: Any) -> list[str]:
    """Return the list of spec slugs created in this project so far."""
    if ctx.memory is None:
        return []
    raw = await ctx.memory.recall(MEMORY_KEY_CREATED_SPECS)
    if not raw:
        return []
    try:
        return json.loads(raw)
    except (json.JSONDecodeError, TypeError):
        return []


async def record_created_spec(ctx: Any, slug: str) -> None:
    """Append *slug* to the project's created-specs list in semantic memory.

    Idempotent: does nothing if *slug* is already in the list.
    """
    if ctx.memory is None:
        return
    current = await ctx.memory.recall(MEMORY_KEY_CREATED_SPECS)
    specs: list[str] = []
    if current:
        try:
            specs = json.loads(current)
        except (json.JSONDecodeError, TypeError):
            specs = []
    if slug not in specs:
        specs.append(slug)
        await ctx.memory.remember(
            key=MEMORY_KEY_CREATED_SPECS,
            value=json.dumps(specs),
            source=_MEMORY_SOURCE,
            confidence=_MEMORY_CONFIDENCE_SPECS,
        )


# ---------------------------------------------------------------------------
# TaskSpec file writing
# ---------------------------------------------------------------------------

async def write_task_spec(ctx: Any, slug: str, content: str) -> bool:
    """Write *content* to ``.apollia/tasks/{slug}.md``.

    Creates the ``.apollia/tasks/`` directory first via ``bash_executor`` when
    that tool is available. Returns ``True`` on success, ``False`` otherwise.
    """
    if ctx.tools is None:
        return False
    path = f".apollia/tasks/{slug}.md"
    try:
        available_tools = ctx.tools.list_tools()
        if "bash_executor" in available_tools:
            await ctx.tools.call(
                "bash_executor", {"cmd": "mkdir -p .apollia/tasks"}
            )
        await ctx.tools.call("file_write", {"path": path, "content": content})
        return True
    except Exception:
        return False


# ---------------------------------------------------------------------------
# Spec block processing
# ---------------------------------------------------------------------------

async def process_spec_blocks(text: str, ctx: Any) -> str:
    """Extract ``[SPEC:slug]…[/SPEC]`` blocks, write the files, clean the text.

    Each matched block is replaced by a confirmation message (on success) or
    a warning (when tools are unavailable). Created slugs are recorded in
    semantic memory for cross-session traceability.
    """
    replacements: list[tuple[str, str]] = []
    for match in _SPEC_BLOCK_RE.finditer(text):
        slug = match.group(1)
        spec_content = match.group(2).strip()
        success = await write_task_spec(ctx, slug, spec_content)
        path = f".apollia/tasks/{slug}.md"
        if success:
            await record_created_spec(ctx, slug)
            msg = f"\n✅ TaskSpec saved: `{path}`\n"
        else:
            msg = f"\n⚠️ Could not save `{path}` (tools unavailable).\n"
        replacements.append((match.group(0), msg))

    result = text
    for original, replacement in replacements:
        result = result.replace(original, replacement, 1)
    return result


# ---------------------------------------------------------------------------
# System prompt construction
# ---------------------------------------------------------------------------

_SYSTEM_PROMPT_TEMPLATE: str = """\
You are **spec-assistant**, an expert in feature design and scope decomposition.

You work for any type of project — software, web, mobile, API, infrastructure, \
business processes — regardless of tech stack. You never generate code.

## Principles

**Understand before specifying.** Spec quality depends on question quality. \
If context is insufficient — especially when no project rules are loaded — use \
the `ask_user` tool to ask structured questions before writing. Batch your questions \
in a single call.

**Specify the what, not the how.** The objective describes the outcome for the user \
or system, not the technical solution. "Users can export to CSV in one click" — \
not "add a GET /export endpoint with format=csv query param".

**Never invent.** Never assume requirements, organization, processes, or constraints \
the user hasn't mentioned and that aren't in the project rules. If something seems \
missing, ask.

**Adapt formalism to context.** A simple UI component doesn't need the same \
decomposition as a database migration. Adjust the number of sections, layers, \
and criteria to what is actually useful.

**Every criterion must be verifiable.** "Works correctly" is not a criterion. \
"The form shows an error message if the email is invalid" is.

## What you do

- Transform an idea, request, or need into a structured **TaskSpec**
- Identify impacted layers and explicit scope
- Define "done" criteria verifiable by a third party
- Integrate project rules when available
- Detect similar existing specs to avoid duplicates
- Refine an existing spec on request

## What you don't do

- Never generate code, snippets, or commands
- Never invent organizational context (team size, roles, processes)
- Never write a spec without enough information — ask questions first

## Output format

When writing a TaskSpec, wrap it with `[SPEC:slug]` and `[/SPEC]` \
(slug lowercase with hyphens, e.g. `user-auth`). The runtime saves \
automatically to `.apollia/tasks/slug.md`. One block per response.

TaskSpec structure is flexible but must contain at minimum: \
objective, layers involved, scope (in/out), and "done" criteria. \
Assumptions, risks, and context sections are only relevant if they add \
real information — don't fill them with generic content.

## Available tools

- `file_read`, `file_write`: read and save specs
- `bash_executor`: explore the workspace (ls, find, git)
- `ask_user`: ask the user structured questions (open, single choice, multi choice) — prefer this when context is lacking

## Project context

{rules_section}

## Existing specs

{specs_section}

## Language

Always respond in the same language as the user's message. Detect their language \
from the input and mirror it naturally.\
"""


def build_system_prompt(
    rules: dict[str, str],
    existing_specs: list[str] | None = None,
) -> str:
    """Build the full system prompt for *lang* with injected project rules.

    Injects a formatted rules section and the list of specs already created
    in this project. Falls back to instructive placeholder messages when
    either source is empty.
    """
    # --- Rules section ---
    raw = rules.get("raw", "").strip()
    forbidden_raw = rules.get("forbidden_deps", "[]")
    try:
        forbidden_list: list[str] = json.loads(forbidden_raw)
    except (json.JSONDecodeError, TypeError):
        forbidden_list = []

    if raw:
        if forbidden_list:
            forbidden_lines = "\n".join(f"- `{d}`" for d in forbidden_list)
            forbidden_str = f"\n{forbidden_lines}"
        else:
            forbidden_str = " (none auto-detected)"
        rules_section = (
            f"**Forbidden dependencies:**{forbidden_str}\n\n"
            f"**Full workspace rules:**\n```\n{raw}\n```"
        )
    else:
        rules_section = (
            "No rules file found in this workspace. "
            "Ask the user about their project constraints and conventions."
        )

    # --- Existing specs section ---
    if existing_specs:
        slugs_str = "\n".join(f"- `{s}`" for s in sorted(existing_specs))
        specs_section = (
            f"The following TaskSpecs already exist in `.apollia/tasks/`:\n{slugs_str}\n\n"
            "Mention these specs if a new request seems similar."
        )
    else:
        specs_section = "No existing specs in this project — this will be the first."

    return _SYSTEM_PROMPT_TEMPLATE.format(
        rules_section=rules_section, specs_section=specs_section,
    )


# ---------------------------------------------------------------------------
# Module-level manifest function (AIP contract)
# ---------------------------------------------------------------------------

def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for spec-assistant."""
    return {
        "name": "spec-assistant",
        "version": "1.0.0",
        "description": (
            "Assistant de conception Apollia OS — transforme n'importe quelle idée "
            "en TaskSpec structurée, actionnable et sauvegardée dans le workspace. "
            "Lit les règles du projet (APOLLIA.md, .apollia/rules.md, …), challenge "
            "l'approche, identifie les couches impactées et définit les critères de "
            "validation. Ne génère jamais de code. "
            "Premier maillon du pipeline spec → dev → review."
        ),
        "execution_mode": "auto",
        "agent_type": "assistant",
        "tools_required": ["file_read", "file_write"],
        "tools_optional": ["bash_executor", "file_list", "ask_user"],
        "tools_requiring_approval": [],
        "packages": [],
        "memory_namespace": "spec-assistant",
        "llm_backend": "precise",
        "supports_streaming": True,
        "supports_a2a": True,
        "step_budget": {"max_steps": 30, "max_tool_calls": 20, "wall_clock_secs": 300},
        "tags": [
            "conception", "specification", "pipeline-dev",
            "taskspec", "no-code", "multi-domaine",
        ],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
        "examples": [
            "Crée une spec pour un système d'authentification JWT avec refresh tokens",
            "Quelles sont les specs en attente dans ce projet ?",
            "Affine la spec user-auth pour ajouter la gestion des rôles",
            "Crée une spec pour l'export CSV de la table Commandes",
            "Y a-t-il déjà une spec similaire avant d'en créer une nouvelle ?",
        ],
        "limitations": [
            "Ne génère jamais de code — uniquement des specs structurées au format TaskSpec",
            "Ne modifie aucun fichier source du projet",
            "Requiert au moins une description fonctionnelle pour démarrer",
            "Requiert file_write pour sauvegarder les specs dans .apollia/tasks/",
        ],
        "setup_notes": (
            "Fonctionne mieux avec un fichier APOLLIA.md (créé par `apollia workspace init`) "
            "ou .apollia/rules.md dans le workspace — les règles et contraintes du projet "
            "sont chargées automatiquement et stockées en mémoire sémantique. "
            "À partir de la deuxième session sur le même projet, les règles sont rechargées "
            "depuis la mémoire sans relire les fichiers. "
            "Sans fichiers de règles, l'assistant pose les questions de clarification au démarrage. "
            "Utilisable de manière autonome, sans les autres assistants du pipeline. "
            "Détecte automatiquement les specs similaires existantes pour éviter les doublons."
        ),
        "skills": [
            {
                "id": "create-spec",
                "name": "Créer une TaskSpec",
                "description": (
                    "Transforme une idée ou demande en TaskSpec structurée et la "
                    "sauvegarde dans `.apollia/tasks/{slug}.md`. "
                    "Pose des questions de clarification si la demande est ambiguë."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "request": {
                        "type": "string",
                        "description": "Description de la feature ou tâche à spécifier",
                        "required": True,
                    },
                    "project_context": {
                        "type": "string",
                        "description": "Contexte projet optionnel (stack, contraintes, …)",
                        "required": False,
                    },
                },
            },
            {
                "id": "refine-spec",
                "name": "Affiner une TaskSpec existante",
                "description": (
                    "Révise et complète une TaskSpec existante en réponse à de "
                    "nouvelles informations, un changement de périmètre ou un retour "
                    "de dev-assistant ou review-assistant."
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
                    "Retourne la liste des TaskSpecs déjà créées dans ce projet "
                    "(depuis la mémoire sémantique et le système de fichiers)."
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

class SpecAssistant(ConversationalAgent):
    """Assistant de conception Apollia OS — premier maillon du pipeline dev.

    Transforms any free-form request into a structured, saved TaskSpec. Never
    generates source code. Adapts to any project type (software, business,
    infrastructure).

    Session startup behaviour (first turn):
    1. Run ``SpecContextBootstrap`` to discover project context (workspace
       rules, tech stack, existing specs) and persist to semantic memory.
    2. Load the snapshot (instant in session N+1 when HEAD hasn't changed).
    3. Parse workspace rules and build a language-specific system prompt.

    Each LLM response is scanned for ``[SPEC:slug]…[/SPEC]`` blocks. Found
    blocks are extracted, written to ``.apollia/tasks/{slug}.md``, recorded in
    semantic memory, and replaced by a one-line confirmation before the
    cleaned text reaches the user.
    """

    SYSTEM_PROMPT: str = _SYSTEM_PROMPT_TEMPLATE.format(
        rules_section="(loaded at session startup)",
        specs_section="(loaded at session startup)",
    )
    MAX_TURNS: int = 30
    TEMPERATURE: float = 0.3

    def __init__(self) -> None:
        self._bootstrap = SpecContextBootstrap()

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for spec-assistant."""
        return manifest()

    async def converse(
        self,
        ctx: Any,
        user_message: str,
        history: list[dict[str, str]] | None = None,
    ) -> tuple[str, list[dict[str, str]]]:
        """Handle one conversational turn.

        On the first turn (empty *history*) the agent runs the context
        bootstrap, loads the snapshot, parses workspace rules, and builds
        a language-specific system prompt. After each LLM response,
        embedded ``[SPEC:slug]`` blocks are written to the workspace before
        the cleaned text is returned.
        """
        if ctx.llm is None:
            raise RuntimeError(
                "SpecAssistant requires ctx.llm — no LLM backend configured"
            )

        is_first_turn = not history

        if is_first_turn:
            if await self._bootstrap.needs_bootstrap(ctx):
                await self._bootstrap.run_bootstrap(ctx)
            snapshot = await self._bootstrap.load_snapshot(ctx)

            raw_rules = (
                snapshot.get("workspace_rules", "")
                if snapshot
                else (
                    ctx.workspace.rules or ""
                    if getattr(ctx, "workspace", None) is not None
                    else ""
                )
            )
            rules = (
                parse_project_rules(raw_rules)
                if raw_rules
                else {"raw": "", "forbidden_deps": "[]", "patterns": "", "comment_convention": ""}
            )

            bootstrap_specs: list[str] = (
                snapshot.get("existing_specs", []) if snapshot else []
            )
            mem_specs = await load_created_specs(ctx)
            existing_specs = sorted(set(bootstrap_specs) | set(mem_specs))

            self.SYSTEM_PROMPT = build_system_prompt(rules, existing_specs)

        messages: list[dict[str, str]] = list(history) if history else []
        if not messages or messages[0].get("role") != "system":
            messages.insert(0, {"role": "system", "content": self.SYSTEM_PROMPT})

        messages.append({"role": "user", "content": user_message})

        response = await ctx.llm.complete(messages)
        raw_text: str = getattr(response, "content", "") or ""

        cleaned_text = await process_spec_blocks(raw_text, ctx)

        messages.append({"role": "assistant", "content": cleaned_text})

        if ctx.memory is not None:
            # Use higher importance when a spec was created in this turn.
            importance = 0.8 if "[SPEC:" in raw_text else 0.4
            await ctx.memory.record(
                content=f"user: {user_message}\nassistant: {cleaned_text}",
                importance=importance,
                task_id=None,
            )

        return cleaned_text, messages

    async def run(self, task: Any, ctx: Any) -> dict[str, Any]:
        """Execute one conversational turn for the given *task*.

        Extracts the user message and conversation history from *task*,
        delegates to :meth:`converse`, and returns an ``AIPResult`` dict.
        """
        if ctx.llm is None:
            return AIPResult.failed(
                "NO_LLM", "SpecAssistant requires ctx.llm — no LLM backend configured"
            )

        task_input = (
            task.get("input") if isinstance(task, dict) else getattr(task, "input", None)
        )
        if task_input is None:
            return AIPResult.failed("NO_INPUT", "No input provided in task")

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

        response_text, _ = await self.converse(ctx, input_text, history=history or None)
        return AIPResult.completed(response_text)


# ---------------------------------------------------------------------------
# Module-level agent instance (required by the Apollia AIP contract)
# ---------------------------------------------------------------------------

agent = SpecAssistant()

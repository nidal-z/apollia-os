"""spec-assistant — Conception and specification assistant for Apollia OS.

First link in the developer pipeline (spec → dev → review). Transforms
free-form requests into structured, actionable TaskSpec files stored in
``.apollia/tasks/{slug}.md`` within the project workspace.

On the first conversational turn, project rules are loaded from semantic
memory (project-scoped per project namespace) or from workspace configuration
files (CLAUDE.md, .apollia/rules.md, …). Rules are persisted to memory so
that subsequent sessions skip file I/O entirely.

LLM responses may embed a ``[SPEC:slug]…[/SPEC]`` block. The agent extracts
it, writes the file, and replaces the block with a confirmation before
returning the cleaned text to the user.

Required tools: file_read, file_write
Optional tools: bash_executor (creates .apollia/tasks/ if needed)
"""

from __future__ import annotations

import json
import re
import unicodedata
from typing import Any

from apollia.agents import AIPResult, ConversationalAgent


# ---------------------------------------------------------------------------
# Memory keys
# ---------------------------------------------------------------------------

MEMORY_KEY_PROJECT_RULES: str = "project_rules"
MEMORY_KEY_FORBIDDEN_DEPS: str = "forbidden_deps"
MEMORY_KEY_PROJECT_PATTERNS: str = "project_patterns"
MEMORY_KEY_COMMENT_CONVENTION: str = "comment_convention"
_MEMORY_SOURCE: str = "spec-assistant"
_MEMORY_CONFIDENCE: float = 0.9

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Workspace rule files probed in declaration order (first found wins, all are
# accumulated).
_RULE_FILES: tuple[str, ...] = (
    ".apollia/rules.md",
    "CLAUDE.md",
    "package.json",
    "Cargo.toml",
    ".eslintrc.json",
    "pyproject.toml",
)

# Maximum characters from project rules injected into the system prompt.
# ~4 000 chars ≈ ~1 200 tokens, leaving comfortable room in context.
_MAX_RULES_CHARS: int = 4_000

# [SPEC:slug]…[/SPEC] — the LLM uses this marker to signal a TaskSpec.
_SPEC_BLOCK_RE: re.Pattern[str] = re.compile(
    r"\[SPEC:([a-z0-9][a-z0-9\-]{0,62})\](.*?)\[/SPEC\]",
    re.DOTALL,
)

_SLUG_NON_ALNUM: re.Pattern[str] = re.compile(r"[^a-z0-9]+")

# Lexical markers used to classify a message as French.
_FRENCH_MARKERS: frozenset[str] = frozenset((
    "bonjour", "salut", "bonsoir", "coucou",
    "je", "tu", "nous", "vous", "il", "elle",
    "est", "sont", "avec", "pour", "dans", "sur",
    "une", "des", "les", "mon", "ton", "son",
    "ajoute", "crée", "fais", "génère", "aide",
    "spec", "fonctionnalité",
    "oui", "non", "merci",
))


# ---------------------------------------------------------------------------
# Language detection
# ---------------------------------------------------------------------------

def _detect_language(text: str) -> str:
    """Return ``"fr"`` when *text* is likely French, ``"en"`` otherwise.

    Uses whole-word token matching only to avoid false positives from
    common substrings (e.g. ``"tu"`` inside ``"feature"``).
    """
    tokens = set(re.split(r"\W+", text.lower()))
    hits = sum(1 for m in _FRENCH_MARKERS if m in tokens)
    return "fr" if hits >= 2 else "en"


# ---------------------------------------------------------------------------
# Slug generation
# ---------------------------------------------------------------------------

def _slugify(title: str) -> str:
    """Convert *title* to a lowercase hyphen-separated slug (max 64 chars)."""
    normalized = unicodedata.normalize("NFD", title.lower())
    ascii_str = normalized.encode("ascii", "ignore").decode("ascii")
    slug = _SLUG_NON_ALNUM.sub("-", ascii_str).strip("-")
    return (slug or "spec")[:64]


# ---------------------------------------------------------------------------
# Project rules parsing
# ---------------------------------------------------------------------------

def _extract_forbidden_deps(raw: str) -> list[str]:
    """Return dependency names explicitly forbidden in *raw*.

    Matches: ``X INTERDIT``, `` `X` INTERDIT ``, ``INTERDIT: X``,
    ``X is forbidden``, ``forbidden: X``, ``no X``, ``banned X``.
    Non-word separators (backticks, parentheses, quotes) between the
    dependency name and the keyword are accepted.
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

    Keys: ``raw``, ``forbidden_deps`` (JSON list), ``patterns``,
    ``comment_convention``.
    """
    forbidden = _extract_forbidden_deps(raw_text)
    patterns = _extract_section_text(
        raw_text,
        "## Patterns obligatoires",
        "### Patterns obligatoires",
        "## Required patterns",
        "### Required patterns",
        "## Règles d'implémentation",
    )
    comment_conv = _extract_section_text(
        raw_text,
        "Convention de commentaires",
        "Comment convention",
        "## Comments",
    )
    truncated = raw_text[:_MAX_RULES_CHARS]
    if len(raw_text) > _MAX_RULES_CHARS:
        truncated += "\n[… règles tronquées pour tenir dans le contexte …]"
    return {
        "raw": truncated,
        "forbidden_deps": json.dumps(forbidden),
        "patterns": patterns[:500],
        "comment_convention": comment_conv[:200],
    }


# ---------------------------------------------------------------------------
# File I/O helpers
# ---------------------------------------------------------------------------

async def _read_file(ctx: Any, path: str) -> str | None:
    """Read *path* via the ``file_read`` tool.

    Returns the file content on success, ``None`` when the file does not
    exist, the tool is unavailable, or an error is reported.
    """
    if ctx.tools is None:
        return None
    try:
        result = await ctx.tools.call("file_read", {"path": path})
        if not isinstance(result, dict):
            return None
        if result.get("error"):
            return None
        content = result.get("content", "")
        return content if content else None
    except Exception:
        return None


# ---------------------------------------------------------------------------
# Project rules loading and persistence
# ---------------------------------------------------------------------------

async def load_project_rules(ctx: Any) -> dict[str, str]:
    """Return project rules for the current workspace.

    Checks semantic memory first (project-scoped). Falls back to reading
    workspace files when no cached rules are found. Returns a dict with
    empty-string values when neither source yields rules.
    """
    _empty: dict[str, str] = {
        "raw": "",
        "forbidden_deps": "[]",
        "patterns": "",
        "comment_convention": "",
    }

    if ctx.memory is not None:
        cached = await ctx.memory.recall(MEMORY_KEY_PROJECT_RULES)
        if cached:
            return {
                "raw": cached,
                "forbidden_deps": await ctx.memory.recall(MEMORY_KEY_FORBIDDEN_DEPS) or "[]",
                "patterns": await ctx.memory.recall(MEMORY_KEY_PROJECT_PATTERNS) or "",
                "comment_convention": await ctx.memory.recall(MEMORY_KEY_COMMENT_CONVENTION) or "",
            }

    chunks: list[str] = []
    for path in _RULE_FILES:
        content = await _read_file(ctx, path)
        if content:
            chunks.append(f"### Source: {path}\n\n{content}")

    if not chunks:
        return _empty

    full_text = "\n\n---\n\n".join(chunks)
    return parse_project_rules(full_text)


async def persist_rules(ctx: Any, rules: dict[str, str]) -> None:
    """Persist *rules* to semantic memory (project-scoped namespace).

    No-ops silently when memory is unavailable or the rules are empty.
    """
    if ctx.memory is None or not rules.get("raw"):
        return
    await ctx.memory.remember(
        key=MEMORY_KEY_PROJECT_RULES,
        value=rules["raw"],
        source=_MEMORY_SOURCE,
        confidence=_MEMORY_CONFIDENCE,
    )
    await ctx.memory.remember(
        key=MEMORY_KEY_FORBIDDEN_DEPS,
        value=rules.get("forbidden_deps", "[]"),
        source=_MEMORY_SOURCE,
        confidence=_MEMORY_CONFIDENCE,
    )
    if rules.get("patterns"):
        await ctx.memory.remember(
            key=MEMORY_KEY_PROJECT_PATTERNS,
            value=rules["patterns"],
            source=_MEMORY_SOURCE,
            confidence=_MEMORY_CONFIDENCE,
        )
    if rules.get("comment_convention"):
        await ctx.memory.remember(
            key=MEMORY_KEY_COMMENT_CONVENTION,
            value=rules["comment_convention"],
            source=_MEMORY_SOURCE,
            confidence=_MEMORY_CONFIDENCE,
        )


# ---------------------------------------------------------------------------
# TaskSpec file writing
# ---------------------------------------------------------------------------

async def write_task_spec(ctx: Any, slug: str, content: str) -> bool:
    """Write *content* to ``.apollia/tasks/{slug}.md``.

    Creates the ``.apollia/tasks/`` directory first if ``bash_executor`` is
    available. Returns ``True`` on success, ``False`` otherwise.
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

async def process_spec_blocks(text: str, ctx: Any, lang: str) -> str:
    """Extract ``[SPEC:slug]…[/SPEC]`` blocks, write the files, clean the text.

    Each matched block is replaced by a one-line confirmation (on success)
    or a warning message (when tools are unavailable).
    """
    replacements: list[tuple[str, str]] = []
    for match in _SPEC_BLOCK_RE.finditer(text):
        slug = match.group(1)
        spec_content = match.group(2).strip()
        success = await write_task_spec(ctx, slug, spec_content)
        path = f".apollia/tasks/{slug}.md"
        if success:
            msg = (
                f"\n✅ TaskSpec sauvegardée : `{path}`\n"
                if lang == "fr"
                else f"\n✅ TaskSpec saved: `{path}`\n"
            )
        else:
            msg = (
                f"\n⚠️ Impossible de sauvegarder `{path}` (outils non disponibles).\n"
                if lang == "fr"
                else f"\n⚠️ Could not save `{path}` (tools unavailable).\n"
            )
        replacements.append((match.group(0), msg))

    result = text
    for original, replacement in replacements:
        result = result.replace(original, replacement, 1)
    return result


# ---------------------------------------------------------------------------
# System prompt construction
# ---------------------------------------------------------------------------

_SYSTEM_PROMPT_TEMPLATE_FR: str = """\
Tu es spec-assistant, assistant de conception du pipeline Apollia OS (spec → dev → review).

## Rôle absolu
Transformer des idées en TaskSpecs structurées. Tu ne génères JAMAIS de code source.
Si l'utilisateur demande du code, de l'implémentation ou de la génération : explique \
que ton rôle est la conception uniquement et redirige vers dev-assistant.

## Processus en 3 étapes
1. **CLARIFIER** (si la demande est ambiguë) : pose des questions ciblées, max 4 par réponse. \
   Objectif : identifier les couches impactées AVANT d'écrire la spec.
2. **RÉDIGER** : une fois les couches claires, rédige la TaskSpec au format standard ci-dessous.
3. **SAUVEGARDER** : encadre la TaskSpec complète avec `[SPEC:le-slug]` … `[/SPEC]`. \
   Le runtime sauvegarde automatiquement dans `.apollia/tasks/le-slug.md`. \
   Le slug est en minuscules avec des tirets (ex. : `user-auth`, `export-button`).

## Format TaskSpec (à utiliser dans [SPEC:slug])

# TaskSpec — {{Titre de la feature}}

> Généré par spec-assistant
> Statut : DRAFT

## Objectif
{{Une phrase — résultat attendu, pas la solution}}

## Couches concernées
- [x] {{Cocher chaque couche réellement impactée}}
- [ ] Base de données / Modèle de données
- [ ] API / Backend / Services
- [ ] Types / Interfaces / Contrats
- [ ] Frontend / UI / Composants
- [ ] Câblage / Intégration
- [ ] Tests unitaires
- [ ] Tests d'intégration / E2E
- [ ] Documentation

## Règles du projet
### Dépendances interdites
{{extraites des règles projet, ou "Aucune"}}
### Patterns obligatoires
{{extraits}}
### Convention de commentaires
{{extraite}}

## Périmètre explicite
### Dans le scope
- {{item}}
### Hors scope
- {{item}}

## Définition de "Fini"
- [ ] {{critère vérifiable par review-assistant}}

## Contexte additionnel

## Historique
- spec créée par spec-assistant

---

## Règles projet chargées pour ce workspace
{rules_section}

---

## Règles de comportement
- Réponds TOUJOURS dans la langue du message utilisateur (FR/EN détecté automatiquement).
- Ne pose pas plus de 4 questions en une seule réponse.
- Toute couche cochée [x] doit avoir au moins un critère dans "Définition de Fini".
- Écris `[SPEC:slug]` immédiatement avant la TaskSpec, `[/SPEC]` immédiatement après.
- Un seul bloc `[SPEC:…]` par réponse.\
"""

_SYSTEM_PROMPT_TEMPLATE_EN: str = """\
You are spec-assistant, the design assistant of the Apollia OS pipeline (spec → dev → review).

## Absolute role
Transform ideas into structured TaskSpecs. You NEVER generate source code.
If the user asks for code, implementation, or generation: explain that your role is \
design only and redirect to dev-assistant.

## 3-step process
1. **CLARIFY** (if the request is ambiguous): ask targeted questions, max 4 per response. \
   Goal: identify impacted layers BEFORE writing the spec.
2. **WRITE**: once the layers are clear, write the TaskSpec in the standard format below.
3. **SAVE**: wrap the complete TaskSpec with `[SPEC:the-slug]` … `[/SPEC]`. \
   The runtime automatically saves to `.apollia/tasks/the-slug.md`. \
   The slug is lowercase with hyphens (e.g. `user-auth`, `export-button`).

## TaskSpec format (to use inside [SPEC:slug])

# TaskSpec — {{Feature title}}

> Generated by spec-assistant
> Status: DRAFT

## Objective
{{One sentence — expected outcome, not the solution}}

## Layers involved
- [x] {{Check each layer actually impacted}}
- [ ] Database / Data model
- [ ] API / Backend / Services
- [ ] Types / Interfaces / Contracts
- [ ] Frontend / UI / Components
- [ ] Wiring / Integration
- [ ] Unit tests
- [ ] Integration / E2E tests
- [ ] Documentation

## Project rules
### Forbidden dependencies
{{extracted from project rules, or "None"}}
### Required patterns
{{extracted}}
### Comment convention
{{extracted}}

## Explicit scope
### In scope
- {{item}}
### Out of scope
- {{item}}

## Definition of Done
- [ ] {{verifiable criterion for review-assistant}}

## Additional context

## History
- spec created by spec-assistant

---

## Project rules loaded for this workspace
{rules_section}

---

## Behaviour rules
- ALWAYS respond in the language of the user's message (auto-detected FR/EN).
- Never ask more than 4 questions in a single response.
- Every checked [x] layer must have at least one criterion in "Definition of Done".
- Write `[SPEC:slug]` immediately before the TaskSpec and `[/SPEC]` immediately after.
- Only one `[SPEC:…]` block per response.\
"""


def build_system_prompt(lang: str, rules: dict[str, str]) -> str:
    """Build the full system prompt for *lang* with injected project rules.

    When no rules are available the prompt instructs the agent to ask the
    user for their project constraints.
    """
    raw = rules.get("raw", "").strip()
    forbidden_raw = rules.get("forbidden_deps", "[]")
    try:
        forbidden_list: list[str] = json.loads(forbidden_raw)
    except (json.JSONDecodeError, TypeError):
        forbidden_list = []

    if raw:
        forbidden_str: str
        if forbidden_list:
            forbidden_str = "\n".join(f"- {d}" for d in forbidden_list)
        else:
            forbidden_str = (
                "(aucune détectée automatiquement)"
                if lang == "fr"
                else "(none auto-detected)"
            )
        if lang == "fr":
            rules_section = (
                f"**Dépendances interdites :**\n{forbidden_str}\n\n"
                f"**Règles complètes :**\n```\n{raw}\n```"
            )
        else:
            rules_section = (
                f"**Forbidden dependencies:**\n{forbidden_str}\n\n"
                f"**Full rules:**\n```\n{raw}\n```"
            )
    else:
        rules_section = (
            "Aucun fichier de règles trouvé dans ce workspace. "
            "Demande à l'utilisateur ses contraintes et préférences projet."
            if lang == "fr"
            else "No rules file found in this workspace. "
            "Ask the user about their project constraints and preferences."
        )

    template = (
        _SYSTEM_PROMPT_TEMPLATE_FR if lang == "fr" else _SYSTEM_PROMPT_TEMPLATE_EN
    )
    return template.format(rules_section=rules_section)


# ---------------------------------------------------------------------------
# Module-level manifest function (AIP contract)
# ---------------------------------------------------------------------------

def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for spec-assistant."""
    return {
        "name": "spec-assistant",
        "version": "1.0.0",
        "description": (
            "Assistant de conception : transforme une idée ou une demande en TaskSpec structurée. "
            "Lit les règles du projet, challenge l'approche, définit les couches concernées "
            "et la définition de 'fini'. Ne génère pas de code. "
            "Premier maillon du pipeline spec → dev → review d'Apollia OS."
        ),
        "execution_mode": "conversational",
        "tools_required": ["file_read", "file_write"],
        "tools_optional": ["bash_executor"],
        "tools_requiring_approval": [],
        "packages": [],
        "memory_namespace": "spec-assistant",
        "supports_streaming": True,
        "supports_a2a": True,
        "step_budget": {"max_steps": 30, "wall_clock_secs": 300},
        "tags": ["dev", "conception", "spec", "pipeline"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
    }


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------

class SpecAssistant(ConversationalAgent):
    """Conception and specification assistant for Apollia OS.

    Drives the first step of the dev pipeline by transforming free-form
    requests into structured TaskSpec files. Never generates source code.

    On the first conversational turn, project rules are loaded from semantic
    memory (project-scoped) or from workspace files, then persisted to memory
    for future sessions. The system prompt is rebuilt per session to embed the
    loaded rules.

    LLM responses embedding ``[SPEC:slug]…[/SPEC]`` blocks trigger automatic
    writes to ``.apollia/tasks/{slug}.md``. The block is replaced with a
    one-line confirmation before the response reaches the user.
    """

    SYSTEM_PROMPT: str = _SYSTEM_PROMPT_TEMPLATE_FR.format(
        rules_section="(rules loaded at runtime)"
    )
    MAX_TURNS: int = 30
    TEMPERATURE: float = 0.3

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

        On the first turn (empty *history*) the agent loads project rules,
        persists them to memory, and builds a language-specific system prompt.
        After each LLM response, embedded ``[SPEC:slug]`` blocks are extracted
        and written to the workspace before the cleaned text is returned.
        """
        if ctx.llm is None:
            raise RuntimeError(
                "SpecAssistant requires ctx.llm — no LLM backend configured"
            )

        lang = _detect_language(user_message)
        is_first_turn = not history

        if is_first_turn:
            self._current_language: str = lang
            rules = await load_project_rules(ctx)
            await persist_rules(ctx, rules)
            self.SYSTEM_PROMPT = build_system_prompt(lang, rules)
        else:
            lang = getattr(self, "_current_language", lang)

        messages: list[dict[str, str]] = list(history) if history else []
        if not messages or messages[0].get("role") != "system":
            messages.insert(0, {"role": "system", "content": self.SYSTEM_PROMPT})

        messages.append({"role": "user", "content": user_message})

        response = await ctx.llm.complete(messages)
        raw_text: str = getattr(response, "content", "") or ""

        cleaned_text = await process_spec_blocks(raw_text, ctx, lang)

        messages.append({"role": "assistant", "content": cleaned_text})

        if ctx.memory is not None:
            await ctx.memory.record(
                content=f"user: {user_message}\nassistant: {cleaned_text}",
                importance=0.4,
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

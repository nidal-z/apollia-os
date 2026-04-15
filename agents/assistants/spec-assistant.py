"""spec-assistant — Assistant de conception pour Apollia OS.

Premier maillon du pipeline de développement (spec → dev → review). Transforme
toute idée ou demande en TaskSpec structurée, sauvegardée dans le workspace du
projet sous ``.apollia/tasks/{slug}.md``.

Fonctionnement :
- Charge les règles projet depuis la mémoire sémantique (scopée par projet) ou
  depuis les fichiers du workspace (APOLLIA.md, .apollia/rules.md, …), puis
  persiste les règles pour éviter toute re-lecture en session suivante.
- Utilise ``memory.search()`` pour détecter les specs similaires existantes et
  prévenir les doublons.
- Enregistre chaque spec créée dans ``created_specs`` pour la traçabilité.
- Les réponses LLM contenant un bloc ``[SPEC:slug]…[/SPEC]`` déclenchent une
  écriture automatique dans le workspace avant retour à l'utilisateur.

Outils requis  : file_read, file_write
Outils optionnels : bash_executor (mkdir), file_list (découverte workspace)
Backend LLM    : precise (qualité de spec > vitesse)
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
MEMORY_KEY_CREATED_SPECS: str = "created_specs"

_MEMORY_SOURCE: str = "spec-assistant"
_MEMORY_CONFIDENCE_RULES: float = 0.9
_MEMORY_CONFIDENCE_SPECS: float = 1.0

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Workspace rule files probed in order. All found files are accumulated.
# APOLLIA.md is the canonical Apollia project rules file (created by `apollia workspace init`).
# CLAUDE.md is kept for compatibility with Claude Code setups.
_RULE_FILES: tuple[str, ...] = (
    "APOLLIA.md",
    ".apollia/rules.md",
    "CLAUDE.md",
    "package.json",
    "Cargo.toml",
    ".eslintrc.json",
    "pyproject.toml",
)

# Maximum characters from project rules injected into the system prompt.
# ~4 000 chars ≈ ~1 200 tokens, leaving comfortable room in any context window.
_MAX_RULES_CHARS: int = 4_000

# [SPEC:slug]…[/SPEC] — the LLM uses this marker to emit a TaskSpec for saving.
_SPEC_BLOCK_RE: re.Pattern[str] = re.compile(
    r"\[SPEC:([a-z0-9][a-z0-9\-]{0,62})\](.*?)\[/SPEC\]",
    re.DOTALL,
)

_SLUG_NON_ALNUM: re.Pattern[str] = re.compile(r"[^a-z0-9]+")

# French whole-word tokens used to detect the user's language.
_FRENCH_MARKERS: frozenset[str] = frozenset((
    "bonjour", "salut", "bonsoir", "coucou",
    "je", "nous", "vous", "il", "elle", "ils",
    "est", "sont", "avec", "pour", "dans", "sur", "par",
    "une", "des", "les", "mon", "ton", "son", "notre",
    "ajoute", "crée", "fais", "génère", "aide", "veux",
    "fonctionnalité", "besoin", "projet",
    "oui", "non", "merci", "svp", "stp",
))


# ---------------------------------------------------------------------------
# Language detection
# ---------------------------------------------------------------------------

def _detect_language(text: str) -> str:
    """Return ``"fr"`` when *text* is likely French, ``"en"`` otherwise.

    Uses whole-word token matching to avoid false positives from common
    substrings (e.g. ``"tu"`` appearing inside ``"feature"``).
    """
    tokens = set(re.split(r"\W+", text.lower()))
    hits = sum(1 for m in _FRENCH_MARKERS if m in tokens)
    return "fr" if hits >= 2 else "en"


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
    exist, the tool is unavailable, or an error is reported by the tool.
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
# Workspace discovery
# ---------------------------------------------------------------------------

async def _list_existing_specs(ctx: Any) -> list[str]:
    """Return slugs of TaskSpec files already present in ``.apollia/tasks/``.

    Uses the ``file_list`` tool when available. Returns an empty list when
    the tool is not configured or the directory does not exist.
    """
    if ctx.tools is None:
        return []
    try:
        available = ctx.tools.list_tools()
        if "file_list" not in available:
            return []
        result = await ctx.tools.call("file_list", {"path": ".apollia/tasks"})
        if not isinstance(result, dict):
            return []
        files = result.get("files", result.get("entries", []))
        slugs: list[str] = []
        for entry in files:
            name = entry if isinstance(entry, str) else entry.get("name", "")
            if name.endswith(".md"):
                slugs.append(name[:-3])
        return slugs
    except Exception:
        return []


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

    No-ops silently when memory is unavailable or the rules dict is empty.
    """
    if ctx.memory is None or not rules.get("raw"):
        return
    await ctx.memory.remember(
        key=MEMORY_KEY_PROJECT_RULES,
        value=rules["raw"],
        source=_MEMORY_SOURCE,
        confidence=_MEMORY_CONFIDENCE_RULES,
    )
    await ctx.memory.remember(
        key=MEMORY_KEY_FORBIDDEN_DEPS,
        value=rules.get("forbidden_deps", "[]"),
        source=_MEMORY_SOURCE,
        confidence=_MEMORY_CONFIDENCE_RULES,
    )
    if rules.get("patterns"):
        await ctx.memory.remember(
            key=MEMORY_KEY_PROJECT_PATTERNS,
            value=rules["patterns"],
            source=_MEMORY_SOURCE,
            confidence=_MEMORY_CONFIDENCE_RULES,
        )
    if rules.get("comment_convention"):
        await ctx.memory.remember(
            key=MEMORY_KEY_COMMENT_CONVENTION,
            value=rules["comment_convention"],
            source=_MEMORY_SOURCE,
            confidence=_MEMORY_CONFIDENCE_RULES,
        )


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

async def process_spec_blocks(text: str, ctx: Any, lang: str) -> str:
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
Tu es **spec-assistant**, assistant de conception du pipeline Apollia OS \
(spec → dev → review).

Tu es expert en définition de features et en découpage de scope. \
Tu travailles pour tout type de projet : logiciel, produit, processus métier, infrastructure.

## Contraintes absolues

1. **Tu ne génères JAMAIS de code source**, de snippets, de commandes ou de configurations \
techniques. Si la demande porte sur l'implémentation, réponds exactement : \
"Mon rôle est la conception uniquement. Pour l'implémentation, utilisez dev-assistant."

2. **Tu ne rédiges une TaskSpec qu'après avoir identifié les couches impactées.** \
Exception : si la demande est déjà complète et non-ambiguë, tu peux sauter les questions.

3. **Toute couche cochée [x] doit avoir au moins un critère dans "Définition de Fini"** — \
sans exception.

4. **Un seul bloc `[SPEC:slug]…[/SPEC]` par réponse.** Le slug est en minuscules avec \
des tirets (ex. : `user-auth`, `export-csv-button`, `pipeline-notification`).

## Processus de travail

### Phase 1 — Évaluation de la demande

**Demande complète** : objectif clair + couches identifiables → rédige directement la TaskSpec.
**Demande partielle** : pose des questions ciblées (max 4 par réponse).
**Demande hors scope** : refuse poliment, explique ton rôle, redirige vers dev-assistant.
**Révision d'une spec existante** : lis le slug mentionné, propose les modifications.

### Phase 2 — Clarification ciblée (si nécessaire)

Ne pose une question que si sa réponse change le contenu de la spec. \
Adapte au type de projet détecté :

**Projet technique (logiciel, API, infra) :**
- Quel est le comportement actuel et quel est le comportement attendu ?
- Quelles interfaces ou couches existantes sont impactées ?
- Y a-t-il des contraintes de performance, sécurité ou compatibilité ?
- Quelle est la condition d'échec / le cas d'erreur principal ?

**Projet métier (processus, organisation, produit) :**
- Qui est l'utilisateur final et quel est son problème aujourd'hui ?
- Quel processus ou outil existant est remplacé ou amélioré ?
- Qui valide que la feature est terminée ?

**Projet généraliste :**
- Quel est le résultat concret attendu (pas la solution, pas le comment) ?
- Quelles dépendances ou préconditions existent ?

Après 2 échanges de clarification sans avancement, rédige la spec avec les \
informations disponibles en notant explicitement les hypothèses.

### Phase 3 — Rédaction de la TaskSpec

Utilise le format exact ci-dessous. Adapte la liste des couches au type de projet. \
Encadre avec `[SPEC:slug]` et `[/SPEC]` — le runtime sauvegarde automatiquement \
dans `.apollia/tasks/slug.md`.

## Format TaskSpec

[SPEC:NOM-DU-SLUG]
# TaskSpec — Titre Lisible de la Feature

> Généré par spec-assistant
> Statut : DRAFT

## Objectif
UNE SEULE PHRASE décrivant le résultat attendu pour l'utilisateur ou le système. \
Pas la solution. Pas le comment. Exemple : "L'utilisateur peut exporter les données \
affichées en CSV d'un seul clic depuis n'importe quelle vue tableau."

## Couches concernées
[Cocher UNIQUEMENT les couches réellement impactées — au moins 2]

Projet technique :
- [ ] Base de données / Modèle de données
- [ ] API / Services backend
- [ ] Types / Interfaces / Contrats
- [ ] Logique métier / Domain layer
- [ ] Frontend / UI / Composants
- [ ] Câblage / Configuration / Intégration
- [ ] Tests unitaires
- [ ] Tests d'intégration / E2E
- [ ] Documentation / Guides

Projet métier (si applicable) :
- [ ] Parties prenantes / Utilisateurs
- [ ] Flux de données / Informations
- [ ] Processus / Workflow
- [ ] Outils / Systèmes impactés
- [ ] Critères de validation / KPIs
- [ ] Documentation / Procédures

## Règles du projet
### Dépendances interdites
LISTER ICI les dépendances extraites des règles projet, ou "Aucune règle spécifique."
### Patterns obligatoires
LISTER ICI les patterns extraits des règles projet, ou "Aucun pattern spécifique."
### Convention de commentaires et documentation
DÉCRIRE ICI la convention extraite, ou "Standard."

## Périmètre explicite
### Dans le scope
- ITEM concret inclus (au moins 2 items)
### Hors scope (explicitement)
- ITEM exclu — ce que cette spec NE couvre PAS (forcer au moins 1 item)

## Définition de "Fini"
[Au moins un critère par couche cochée ci-dessus. Chaque critère doit être vérifiable.]
- [ ] CRITÈRE observable et testable

## Hypothèses et dépendances
[Ce sur quoi cette spec repose. Si une hypothèse est fausse, la spec doit être révisée.]
- HYPOTHÈSE ou DÉPENDANCE

## Risques identifiés
[Risques techniques ou métier pouvant bloquer ou dévier l'implémentation]
- RISQUE : impact potentiel

## Contexte additionnel
Tout élément utile non couvert ailleurs : contraintes de calendrier, décisions \
architecturales préexistantes, liens vers d'autres specs connexes.

## Historique des révisions
- DRAFT : spec créée par spec-assistant
[/SPEC]

---

## Règles projet chargées pour ce workspace

{rules_section}

---

## Specs déjà créées dans ce projet

{specs_section}

---

## Règles de comportement

- Réponds TOUJOURS dans la langue du message de l'utilisateur (FR/EN auto-détecté).
- Sois concis dans les clarifications — une question par ligne, pas de listes numérotées.
- Ne jamais inclure de blocs de code (``` ou ~~~) dans tes réponses.
- Si une spec similaire existe déjà (voir "Specs déjà créées"), mentionne-la avant \
  d'en créer une nouvelle et demande si c'est une révision ou une spec distincte.
- Privilégie la précision à l'exhaustivité : un critère de "Définition de Fini" doit \
  être vérifiable par une personne tierce, pas une intention vague.\
"""

_SYSTEM_PROMPT_TEMPLATE_EN: str = """\
You are **spec-assistant**, the design assistant of the Apollia OS pipeline \
(spec → dev → review).

You are an expert in feature definition and scope decomposition. \
You work for any type of project: software, product, business process, infrastructure.

## Absolute constraints

1. **You NEVER generate source code**, snippets, commands, or technical configurations. \
If the request is about implementation, reply exactly: \
"My role is design only. For implementation, use dev-assistant."

2. **You only write a TaskSpec after identifying the impacted layers.** \
Exception: if the request is already complete and unambiguous, skip the questions.

3. **Every checked [x] layer must have at least one criterion in "Definition of Done"** — \
no exceptions.

4. **One single `[SPEC:slug]…[/SPEC]` block per response.** The slug is lowercase with \
hyphens (e.g. `user-auth`, `export-csv-button`, `pipeline-notification`).

## Work process

### Phase 1 — Request evaluation

**Complete request**: clear objective + identifiable layers → write the TaskSpec directly.
**Partial request**: ask targeted questions (max 4 per response).
**Out-of-scope request**: politely decline, explain your role, redirect to dev-assistant.
**Existing spec revision**: read the mentioned slug, propose changes.

### Phase 2 — Targeted clarification (if needed)

Only ask a question if its answer changes the spec content. \
Adapt to the detected project type:

**Technical project (software, API, infra):**
- What is the current behaviour and what is the expected behaviour?
- Which existing interfaces or layers are impacted?
- Are there performance, security, or compatibility constraints?
- What is the main failure condition / error case?

**Business project (process, organisation, product):**
- Who is the end user and what is their problem today?
- Which existing process or tool is being replaced or improved?
- Who validates that the feature is done?

**General project:**
- What is the concrete expected outcome (not the solution, not the how)?
- What dependencies or preconditions exist?

After 2 clarification exchanges without progress, write the spec with available \
information and explicitly note the assumptions.

### Phase 3 — TaskSpec writing

Use the exact format below. Adapt the layer list to the project type. \
Wrap with `[SPEC:slug]` and `[/SPEC]` — the runtime saves automatically \
to `.apollia/tasks/slug.md`.

## TaskSpec format

[SPEC:SLUG-NAME]
# TaskSpec — Readable Feature Title

> Generated by spec-assistant
> Status: DRAFT

## Objective
ONE SINGLE SENTENCE describing the expected outcome for the user or system. \
Not the solution. Not the how. Example: "Users can export displayed data to CSV \
in one click from any table view."

## Layers involved
[Check ONLY layers that are actually impacted — at least 2]

Technical project:
- [ ] Database / Data model
- [ ] API / Backend services
- [ ] Types / Interfaces / Contracts
- [ ] Business logic / Domain layer
- [ ] Frontend / UI / Components
- [ ] Wiring / Configuration / Integration
- [ ] Unit tests
- [ ] Integration / E2E tests
- [ ] Documentation / Guides

Business project (if applicable):
- [ ] Stakeholders / Users
- [ ] Data flows / Information
- [ ] Process / Workflow
- [ ] Tools / Systems impacted
- [ ] Validation criteria / KPIs
- [ ] Documentation / Procedures

## Project rules
### Forbidden dependencies
LIST HERE dependencies extracted from project rules, or "None specific."
### Required patterns
LIST HERE patterns extracted from project rules, or "None specific."
### Comment and documentation convention
DESCRIBE HERE the convention extracted, or "Standard."

## Explicit scope
### In scope
- CONCRETE item included (at least 2 items)
### Out of scope (explicitly)
- EXCLUDED item — what this spec does NOT cover (force at least 1 item)

## Definition of Done
[At least one criterion per checked layer above. Each criterion must be verifiable.]
- [ ] OBSERVABLE and testable criterion

## Assumptions and dependencies
[What this spec relies on. If an assumption is false, the spec must be revised.]
- ASSUMPTION or DEPENDENCY

## Identified risks
[Technical or business risks that could block or derail implementation]
- RISK: potential impact

## Additional context
Any useful element not covered elsewhere: timeline constraints, existing architectural \
decisions, links to related specs.

## Revision history
- DRAFT: spec created by spec-assistant
[/SPEC]

---

## Project rules loaded for this workspace

{rules_section}

---

## Specs already created in this project

{specs_section}

---

## Behaviour rules

- ALWAYS respond in the language of the user's message (auto-detected FR/EN).
- Be concise in clarifications — one question per line, no numbered lists.
- Never include code fences (``` or ~~~) in your responses.
- If a similar spec already exists (see "Specs already created"), mention it before \
  creating a new one and ask whether it is a revision or a distinct spec.
- Prioritise precision over exhaustiveness: a "Definition of Done" criterion must be \
  verifiable by a third party, not a vague intention.\
"""


def build_system_prompt(
    lang: str,
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
            forbidden_str = (
                " (aucune détectée automatiquement)"
                if lang == "fr"
                else " (none auto-detected)"
            )
        if lang == "fr":
            rules_section = (
                f"**Dépendances interdites :**{forbidden_str}\n\n"
                f"**Règles complètes du workspace :**\n```\n{raw}\n```"
            )
        else:
            rules_section = (
                f"**Forbidden dependencies:**{forbidden_str}\n\n"
                f"**Full workspace rules:**\n```\n{raw}\n```"
            )
    else:
        rules_section = (
            "Aucun fichier de règles trouvé dans ce workspace. "
            "Interroge l'utilisateur sur ses contraintes et conventions projet."
            if lang == "fr"
            else "No rules file found in this workspace. "
            "Ask the user about their project constraints and conventions."
        )

    # --- Existing specs section ---
    if existing_specs:
        slugs_str = "\n".join(f"- `{s}`" for s in sorted(existing_specs))
        specs_section = (
            f"Les TaskSpecs suivantes existent déjà dans `.apollia/tasks/` :\n{slugs_str}\n\n"
            "Mentionne ces specs si une nouvelle demande semble similaire."
            if lang == "fr"
            else f"The following TaskSpecs already exist in `.apollia/tasks/`:\n{slugs_str}\n\n"
            "Mention these specs if a new request seems similar."
        )
    else:
        specs_section = (
            "Aucune spec existante dans ce projet — c'est la première."
            if lang == "fr"
            else "No existing specs in this project — this will be the first."
        )

    template = (
        _SYSTEM_PROMPT_TEMPLATE_FR if lang == "fr" else _SYSTEM_PROMPT_TEMPLATE_EN
    )
    return template.format(rules_section=rules_section, specs_section=specs_section)


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
        "tools_optional": ["bash_executor", "file_list"],
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
    1. Load project rules from memory (project-scoped) or workspace files.
    2. Discover existing specs via ``file_list`` (if available) and memory.
    3. Persist rules to memory for future sessions.
    4. Build a language-specific system prompt embedding rules and existing specs.

    Each LLM response is scanned for ``[SPEC:slug]…[/SPEC]`` blocks. Found
    blocks are extracted, written to ``.apollia/tasks/{slug}.md``, recorded in
    semantic memory, and replaced by a one-line confirmation before the
    cleaned text reaches the user.
    """

    SYSTEM_PROMPT: str = _SYSTEM_PROMPT_TEMPLATE_FR.format(
        rules_section="(chargées au démarrage de la session)",
        specs_section="(chargées au démarrage de la session)",
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
        discovers existing specs, persists everything to memory, and builds
        a language-specific system prompt. After each LLM response,
        embedded ``[SPEC:slug]`` blocks are written to the workspace before
        the cleaned text is returned.
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

            # Merge file-system discovery with memory-tracked specs.
            fs_specs = await _list_existing_specs(ctx)
            mem_specs = await load_created_specs(ctx)
            existing_specs = sorted(set(fs_specs) | set(mem_specs))

            self.SYSTEM_PROMPT = build_system_prompt(lang, rules, existing_specs)
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

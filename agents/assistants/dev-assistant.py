"""dev-assistant — Assistant d'implémentation pour Apollia OS.

Deuxième maillon du pipeline de développement (spec → dev → review). Prend une
TaskSpec existante et l'implémente couche par couche, en respectant les règles
projet et sans jamais commencer sans contrat pré-tâche validé.

Fonctionnement :
- Utilise ``DevContextBootstrap`` pour charger le contexte projet (workspace
  rules, tech stack, architecture, fichiers récents) et le persister en mémoire
  sémantique.  En session N+1, le snapshot est rechargé sans relire les fichiers.
- Détecte l'intention (implémentation vs exploration) depuis le message utilisateur.
- Mode implémentation : charge ou crée une TaskSpec minimale, demande validation
  si aucune n'existe, puis implémente chaque couche cochée [x] via A2A → code-worker.
  Met à jour la TaskSpec au fur et à mesure pour traçabilité.
- Mode exploration : utilise le LLM pour répondre aux questions sur la codebase
  depuis la mémoire sémantique ou les fichiers, sans proposer d'implémentation
  spontanée.
- Mémorise les résultats d'exploration pour les sessions suivantes
  (clé ``codebase:{slug}``).

Outils requis  : file_read, file_write
Outils optionnels : bash_executor (mkdir, git, find), file_list (découverte workspace)
Outils avec approbation : bash_executor
Backend LLM    : precise (mode exploration uniquement)
A2A            : code-worker (implémentation), git-worker (commit optionnel)
"""

from __future__ import annotations

import json
import re
import unicodedata
from typing import Any

from apollia.agents import AIPResult, ConversationalAgent

from assistants.shared.project_bootstrap import ProjectContextBootstrap


# ---------------------------------------------------------------------------
# Memory keys — project-scoped
# ---------------------------------------------------------------------------

MEMORY_KEY_CODEBASE_PREFIX: str = "codebase:"

_MEMORY_SOURCE: str = "dev-assistant"
_MEMORY_CONFIDENCE_CODEBASE: float = 0.7


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

_TASK_SPEC_DIR: str = ".apollia/tasks"

# Marqueurs de couche dans une TaskSpec
_CHECKED_LAYER_RE: re.Pattern[str] = re.compile(
    r"^-\s+\[x\]\s+(.+?)(?:\s+✅)?\s*$", re.MULTILINE | re.IGNORECASE
)
_UNCHECKED_LAYER_RE: re.Pattern[str] = re.compile(
    r"^-\s+\[ \]\s+(.+)$", re.MULTILINE
)

# Référence de spec dans un message utilisateur
_SPEC_REF_RE: re.Pattern[str] = re.compile(
    r"(?:\.apollia/tasks/|taskspec\s+|spec(?:ification)?\s+[:\s]?\s*)"
    r"([a-z0-9][a-z0-9\-]{0,62})(?:\.md)?",
    re.IGNORECASE,
)

_SLUG_NON_ALNUM: re.Pattern[str] = re.compile(r"[^a-z0-9]+")

# Marqueurs de langue française (correspondance sur token entier)
_FRENCH_MARKERS: frozenset[str] = frozenset((
    "bonjour", "salut", "bonsoir", "coucou",
    "je", "nous", "vous", "il", "elle", "ils",
    "est", "sont", "avec", "pour", "dans", "sur", "par",
    "une", "un", "le", "la", "les", "des", "du",
    "mon", "ton", "son", "notre", "de",
    "implémente", "implémenter", "ajoute", "crée", "fais", "génère",
    "comment", "pourquoi", "quoi", "oui", "non", "merci",
    "système", "fonctionnalité", "service", "projet", "besoin",
))

# Marqueurs d'intention d'implémentation
_IMPLEMENT_MARKERS: frozenset[str] = frozenset((
    "implement", "implémente", "implémenter",
    "code", "coder", "create", "crée", "créer",
    "add", "ajoute", "ajouter", "build", "construis",
    "develop", "développe", "développer", "write", "écris",
    "generate", "génère", "make", "fais", "start", "commence",
))

# Marqueurs d'intention d'exploration
_EXPLORE_MARKERS: frozenset[str] = frozenset((
    "how", "comment", "what", "quoi",
    "explain", "explique", "describe", "décris",
    "show", "montre", "find", "trouve",
    "why", "pourquoi", "where", "où",
    "understand", "comprendre", "works", "fonctionne",
    "liste", "list", "affiche", "display",
))


# ---------------------------------------------------------------------------
# Context Bootstrap
# ---------------------------------------------------------------------------


class DevContextBootstrap(ProjectContextBootstrap):
    """Bootstrap for dev-assistant.

    Extends the common project snapshot with:

    - ``architecture``: modules/crates/services detected in the workspace
    - ``recent_files``: files modified since HEAD~10
    """

    async def extra_scopes(
        self,
        ctx: Any,
        base_snapshot: dict[str, Any],
    ) -> dict[str, Any]:
        """Discover architecture modules and recently modified files."""
        modules: list[str] = []
        recent: list[str] = []

        if ctx.tools is None:
            return {"architecture": modules, "recent_files": recent}

        arch_result = await ctx.tools.call("bash_executor", {
            "command": (
                "find . -maxdepth 3 \\( "
                "-name 'Cargo.toml' -o -name 'mod.rs' "
                "-o -name '__init__.py' -o -name 'index.ts' "
                "\\) 2>/dev/null | grep -v target | head -40 | sort"
            ),
        })
        if arch_result and arch_result.get("stdout"):
            raw = arch_result["stdout"]
            if isinstance(raw, str):
                modules = [line.strip() for line in raw.split("\n") if line.strip()]

        recent_result = await ctx.tools.call("bash_executor", {
            "command": (
                "git diff --name-only HEAD~10 HEAD 2>/dev/null "
                "|| find . -maxdepth 3 -newer .git/HEAD 2>/dev/null | head -30"
            ),
        })
        if recent_result and recent_result.get("stdout"):
            raw = recent_result["stdout"]
            if isinstance(raw, str):
                recent = [line.strip() for line in raw.split("\n") if line.strip()]

        return {"architecture": modules, "recent_files": recent}


# ---------------------------------------------------------------------------
# Language detection
# ---------------------------------------------------------------------------

def _detect_language(text: str) -> str:
    """Retourne ``"fr"`` si *text* est probablement en français, ``"en"`` sinon.

    Utilise une correspondance sur token entier pour éviter les faux positifs
    liés aux sous-chaînes communes (ex. ``"tu"`` dans ``"feature"``).
    """
    tokens = set(re.split(r"\W+", text.lower()))
    hits = sum(1 for m in _FRENCH_MARKERS if m in tokens)
    return "fr" if hits >= 2 else "en"


# ---------------------------------------------------------------------------
# Intent detection
# ---------------------------------------------------------------------------

def _detect_intent(text: str) -> str:
    """Retourne ``"implement"`` ou ``"explore"`` selon l'intention détectée.

    Une question (``?``) ou des marqueurs d'exploration font pencher vers
    ``"explore"``. Des marqueurs d'implémentation explicites font pencher
    vers ``"implement"``. Le défaut est ``"explore"`` pour éviter toute
    implémentation involontaire.
    """
    tokens = set(re.split(r"\W+", text.lower()))
    impl_hits = sum(1 for m in _IMPLEMENT_MARKERS if m in tokens)
    expl_hits = sum(1 for m in _EXPLORE_MARKERS if m in tokens)

    if "?" in text:
        expl_hits += 2

    if impl_hits > expl_hits:
        return "implement"
    return "explore"


# ---------------------------------------------------------------------------
# Slug generation
# ---------------------------------------------------------------------------

def _slugify(title: str) -> str:
    """Convertit *title* en slug ASCII minuscule (64 caractères max)."""
    normalized = unicodedata.normalize("NFD", title.lower())
    ascii_str = normalized.encode("ascii", "ignore").decode("ascii")
    slug = _SLUG_NON_ALNUM.sub("-", ascii_str).strip("-")
    return (slug or "task")[:64]


# ---------------------------------------------------------------------------
# Section extraction
# ---------------------------------------------------------------------------

def _extract_section_text(raw: str, *headers: str) -> str:
    """Retourne le bloc de texte qui suit immédiatement le premier *header* trouvé."""
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


# ---------------------------------------------------------------------------
# TaskSpec parsing
# ---------------------------------------------------------------------------

def extract_layers_to_implement(content: str) -> list[str]:
    """Retourne les noms de couches marquées ``[x]`` dans "Couches concernées".

    Ces couches sont dans le périmètre de la feature et doivent être
    implémentées. Les couches déjà marquées ✅ sont exclues (déjà traitées).
    """
    section = _extract_section_text(
        content,
        "## Couches concernées",
        "## Layers",
        "## Couches",
    )
    source = section if section else content
    layers = [
        m.group(1).strip()
        for m in _CHECKED_LAYER_RE.finditer(source)
        if "✅" not in m.group(0)
    ]
    return layers


def extract_objective(content: str) -> str:
    """Retourne l'objectif décrit dans la section ``## Objectif`` de la TaskSpec."""
    section = _extract_section_text(
        content, "## Objectif", "## Objective", "## Goal"
    )
    return section.strip()[:500] if section else "Feature implementation"


def mark_layer_implemented(content: str, layer: str) -> str:
    """Marque la couche *layer* comme implémentée (ajoute ``✅``) dans *content*.

    La modification porte sur la première occurrence de ``[x] {layer}``
    sans ✅ existant.
    """
    target = f"[x] {layer}"
    replacement = f"[x] {layer} ✅"
    idx = content.find(target)
    if idx == -1:
        return content
    after = content[idx + len(target):]
    if after.startswith(" ✅"):
        return content
    return content[:idx] + replacement + after


# ---------------------------------------------------------------------------
# Project rules parsing
# ---------------------------------------------------------------------------


def _parse_forbidden_deps(raw: str) -> list[str]:
    """Extrait les noms de dépendances explicitement interdites dans *raw*."""
    patterns = [
        r"\b([a-zA-Z][\w\-]+)\b\W+INTERDIT\b",
        r"INTERDIT[^:]*:\s*([a-zA-Z][\w\-]+)",
        r"\b([a-zA-Z][\w\-]+)\b\W+interdit\b",
        r"forbidden[^:]*:\s*([a-zA-Z][\w\-]+)",
        r"\b([a-zA-Z][\w\-]+)\b\W+is\s+forbidden\b",
        r"\b([a-zA-Z][\w\-]+)\b\W+not\s+allowed\b",
        r"\bno\s+([a-zA-Z][\w\-]+)\b",
    ]
    found: set[str] = set()
    for pat in patterns:
        for m in re.finditer(pat, raw):
            dep = m.group(1).strip()
            if len(dep) >= 2:
                found.add(dep)
    return sorted(found)


def _parse_rules(raw_text: str) -> dict[str, str]:
    """Parse *raw_text* et retourne les règles catégorisées.

    Retourne un dict avec les clés ``raw`` (texte complet tronqué),
    ``forbidden_deps`` (JSON list), ``patterns``, ``comment_convention``.
    """
    forbidden = _parse_forbidden_deps(raw_text)
    patterns = _extract_section_text(
        raw_text,
        "## Patterns obligatoires",
        "### Patterns obligatoires",
        "## Required patterns",
        "## Règles d'implémentation",
    )
    comment_conv = _extract_section_text(
        raw_text,
        "Convention de commentaires",
        "Comment convention",
        "## Comments",
    )
    truncated = raw_text[:4_000]
    if len(raw_text) > 4_000:
        truncated += "\n[… règles tronquées …]"
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
    """Lit *path* via l'outil ``file_read``.

    Retourne le contenu en cas de succès, ``None`` si le fichier n'existe pas,
    si l'outil est indisponible ou si une erreur est signalée.
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
# TaskSpec file operations
# ---------------------------------------------------------------------------

async def load_task_spec(ctx: Any, slug: str) -> str | None:
    """Charge le contenu de ``.apollia/tasks/{slug}.md``.

    Retourne le contenu Markdown en cas de succès, ``None`` sinon.
    """
    path = f"{_TASK_SPEC_DIR}/{slug}.md"
    return await _read_file(ctx, path)


async def list_task_specs(ctx: Any) -> list[str]:
    """Retourne les slugs des TaskSpecs présentes dans ``.apollia/tasks/``.

    Utilise ``file_list`` si disponible, retourne une liste vide sinon.
    """
    if ctx.tools is None:
        return []
    try:
        available = ctx.tools.list_tools()
        if "file_list" not in available:
            return []
        result = await ctx.tools.call("file_list", {"path": _TASK_SPEC_DIR})
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


async def write_task_spec(ctx: Any, slug: str, content: str) -> bool:
    """Écrit *content* dans ``.apollia/tasks/{slug}.md``.

    Crée le répertoire ``.apollia/tasks/`` via ``bash_executor`` si disponible.
    Retourne ``True`` en cas de succès, ``False`` sinon.
    """
    if ctx.tools is None:
        return False
    path = f"{_TASK_SPEC_DIR}/{slug}.md"
    try:
        available = ctx.tools.list_tools()
        if "bash_executor" in available:
            await ctx.tools.call(
                "bash_executor", {"cmd": f"mkdir -p {_TASK_SPEC_DIR}"}
            )
        await ctx.tools.call("file_write", {"path": path, "content": content})
        return True
    except Exception:
        return False


def create_minimal_task_spec(slug: str, objective: str, rules: dict[str, str]) -> str:
    """Génère une TaskSpec minimale à partir de l'objectif et des règles projet.

    La spec créée contient des couches par défaut non cochées. L'utilisateur
    doit cocher les couches pertinentes et valider avant que dev-assistant
    commence l'implémentation.
    """
    title = slug.replace("-", " ").title()
    forbidden_raw = rules.get("forbidden_deps", "[]")
    try:
        forbidden_list: list[str] = json.loads(forbidden_raw)
    except (json.JSONDecodeError, TypeError):
        forbidden_list = []

    forbidden_section = (
        "\n".join(f"- `{d}`" for d in forbidden_list)
        if forbidden_list
        else "Aucune règle spécifique."
    )
    patterns_section = rules.get("patterns", "") or "Aucun pattern spécifique."

    return f"""\
# TaskSpec — {title}

> Généré par dev-assistant
> Statut : DRAFT — À valider avant implémentation

## Objectif
{objective}

## Couches concernées
Cochez [x] les couches à implémenter, laissez [ ] les autres.

- [ ] Base de données / Modèle de données
- [ ] API / Services backend
- [ ] Types / Interfaces / Contrats
- [ ] Logique métier / Domain layer
- [ ] Frontend / UI / Composants
- [ ] Câblage / Configuration / Intégration
- [ ] Tests unitaires
- [ ] Tests d'intégration / E2E
- [ ] Documentation / Guides

## Règles du projet
### Dépendances interdites
{forbidden_section}
### Patterns obligatoires
{patterns_section}

## Périmètre explicite
### Dans le scope
- À préciser avec l'utilisateur

### Hors scope (explicitement)
- À préciser avec l'utilisateur

## Définition de "Fini"
- [ ] Les couches cochées sont implémentées
- [ ] Les tests passent sans régression

## Hypothèses et dépendances
- Périmètre à confirmer avec l'utilisateur

## Risques identifiés
- Scope non défini — risque de dérive d'implémentation

## Historique des révisions
- DRAFT : spec minimale créée par dev-assistant
"""


# ---------------------------------------------------------------------------
# Spec slug extraction from user message
# ---------------------------------------------------------------------------

def _extract_spec_slug(text: str) -> str | None:
    """Extrait un slug de TaskSpec depuis *text* si présent.

    Recherche des patterns comme ``.apollia/tasks/jwt-auth`` ou
    ``spec jwt-auth`` ou ``taskspec jwt-auth.md``.
    """
    match = _SPEC_REF_RE.search(text)
    if match:
        return match.group(1).lower()
    return None


# ---------------------------------------------------------------------------
# A2A delegation
# ---------------------------------------------------------------------------

async def delegate_to_code_worker(
    ctx: Any,
    layer: str,
    objective: str,
    spec_content: str,
    rules: dict[str, str],
) -> str:
    """Délègue l'implémentation d'une couche à ``code-worker`` via A2A.

    Retourne la sortie de code-worker, ou un message de repli si A2A n'est
    pas disponible dans ce contexte d'exécution.
    """
    if not (hasattr(ctx, "a2a") and ctx.a2a is not None):
        return (
            f"[A2A non disponible] La couche '{layer}' doit être implémentée "
            f"manuellement selon la TaskSpec."
        )

    try:
        forbidden_raw = rules.get("forbidden_deps", "[]")
        try:
            forbidden_list: list[str] = json.loads(forbidden_raw)
        except (json.JSONDecodeError, TypeError):
            forbidden_list = []

        result = await ctx.a2a.call(
            agent="code-worker",
            skill="implement",
            input={
                "task": f"Implement layer '{layer}' for: {objective}",
                "spec": spec_content,
                "forbidden_deps": forbidden_list,
                "comment_convention": rules.get("comment_convention", ""),
                "patterns": rules.get("patterns", ""),
            },
        )
        return str(result.get("output", f"Layer '{layer}' delegated to code-worker."))
    except Exception as exc:
        return f"[code-worker error] {exc}"


async def delegate_to_git_worker(
    ctx: Any,
    commit_message: str,
    files: list[str],
) -> dict[str, Any] | None:
    """Délègue la création d'un commit à ``git-worker`` via A2A.

    Retourne le résultat de git-worker, ou ``None`` si A2A est indisponible.
    """
    if not (hasattr(ctx, "a2a") and ctx.a2a is not None):
        return None
    try:
        result = await ctx.a2a.call(
            agent="git-worker",
            skill="commit",
            input={"message": commit_message, "files": files},
        )
        return dict(result)
    except Exception:
        return None


# ---------------------------------------------------------------------------
# System prompt construction
# ---------------------------------------------------------------------------

_EXPLORE_SYSTEM_PROMPT_FR: str = """\
Tu es **dev-assistant** en mode **exploration**.

Tu réponds aux questions sur la codebase, les patterns d'implémentation \
et l'architecture du projet. Tu te bases sur ta mémoire sémantique ou sur la \
lecture des fichiers du workspace.

## Contraintes absolues

1. **Tu ne proposes JAMAIS d'implémenter, modifier ou créer du code** à moins que \
l'utilisateur ne demande explicitement "implémente" et ne fournisse ou mentionne \
une TaskSpec valide dans `.apollia/tasks/`.
2. Si l'utilisateur veut implémenter : réponds exactement \
"Pour implémenter, précise la TaskSpec à utiliser (.apollia/tasks/slug.md) \
ou reformule avec 'implémente [feature]'."
3. Réponds de façon concise et factuelle.
4. Ne génère jamais de blocs de code complets (``` ou ~~~) — \
seulement des extraits courts pour illustrer un pattern existant.
5. Les seuls commentaires dans le code que tu cites expliquent une logique non-évidente \
— jamais de commentaires qui répètent ce que le nom de la fonction dit déjà.

## Règles projet

{rules_section}\
"""

_EXPLORE_SYSTEM_PROMPT_EN: str = """\
You are **dev-assistant** in **exploration mode**.

You answer questions about the codebase, implementation patterns, and project \
architecture. You rely on your semantic memory or read workspace files.

## Absolute constraints

1. **You NEVER propose to implement, modify, or create code** unless the user \
explicitly says "implement" and provides or mentions a valid TaskSpec in \
`.apollia/tasks/`.
2. If the user wants to implement, reply exactly: \
"To implement, specify the TaskSpec to use (.apollia/tasks/slug.md) \
or rephrase with 'implement [feature]'."
3. Respond concisely and factually.
4. Never generate full code blocks (``` or ~~~) — only short excerpts \
to illustrate an existing pattern.
5. Comments in code you quote must explain non-obvious logic only — \
never repeat what the function name already says.

## Project rules

{rules_section}\
"""


def build_explore_system_prompt(lang: str, rules: dict[str, str]) -> str:
    """Construit le system prompt du mode exploration avec les règles projet injectées."""
    raw = rules.get("raw", "").strip()

    if raw:
        forbidden_raw = rules.get("forbidden_deps", "[]")
        try:
            forbidden_list: list[str] = json.loads(forbidden_raw)
        except (json.JSONDecodeError, TypeError):
            forbidden_list = []

        if lang == "fr":
            parts = [
                f"**Dépendances interdites :** "
                f"{', '.join(f'`{d}`' for d in forbidden_list) or 'aucune détectée'}"
            ]
            if rules.get("patterns"):
                parts.append(f"**Patterns obligatoires :** {rules['patterns'][:300]}")
            rules_section = "\n\n".join(parts)
        else:
            parts = [
                f"**Forbidden dependencies:** "
                f"{', '.join(f'`{d}`' for d in forbidden_list) or 'none detected'}"
            ]
            if rules.get("patterns"):
                parts.append(f"**Required patterns:** {rules['patterns'][:300]}")
            rules_section = "\n\n".join(parts)
    else:
        rules_section = (
            "Aucun fichier de règles trouvé dans ce workspace."
            if lang == "fr"
            else "No rules file found in this workspace."
        )

    template = (
        _EXPLORE_SYSTEM_PROMPT_FR if lang == "fr" else _EXPLORE_SYSTEM_PROMPT_EN
    )
    return template.format(rules_section=rules_section)


# ---------------------------------------------------------------------------
# Task input extraction
# ---------------------------------------------------------------------------

def _extract_task_input(task: Any) -> tuple[str, list[dict[str, str]]]:
    """Extrait le texte d'entrée et l'historique depuis *task*.

    Supporte les dicts (format API), les objets avec attributs (format runtime
    PyO3), et les parties A2A multi-turn.
    Retourne un tuple ``(input_text, history)``.
    """
    task_input = (
        task.get("input") if isinstance(task, dict) else getattr(task, "input", None)
    )

    if task_input is None:
        return "", []

    if isinstance(task_input, dict):
        parts = task_input.get("parts", [])
        if parts and isinstance(parts[0], dict):
            input_text: str = parts[0].get("text", "")
        else:
            input_text = str(task_input.get("text", ""))
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
            msg_parts = msg.get("parts", [])
            text = (
                msg_parts[0]["text"]
                if msg_parts and isinstance(msg_parts[0], dict)
                else str(msg.get("text", msg))
            )
            history.append({"role": role, "content": text})
        elif hasattr(msg, "role"):
            role = "assistant" if msg.role == "agent" else msg.role
            msg_parts = getattr(msg, "parts", [])
            text = msg_parts[0].text if msg_parts else str(msg)
            history.append({"role": role, "content": text})

    return input_text, history


# ---------------------------------------------------------------------------
# Module-level manifest function (AIP contract)
# ---------------------------------------------------------------------------

def manifest() -> dict[str, Any]:
    """Retourne le manifest AIP de dev-assistant."""
    return {
        "name": "dev-assistant",
        "version": "1.0.0",
        "description": (
            "Assistant d'implémentation Apollia OS — prend une TaskSpec et implémente "
            "couche par couche en respectant les règles du projet. "
            "Contrat pré-tâche obligatoire : refuse de commencer sans TaskSpec validée. "
            "Délègue à code-worker et git-worker via A2A. "
            "Aucun commentaire évident dans le code produit. "
            "Deuxième maillon du pipeline spec → dev → review."
        ),
        "execution_mode": "auto",
        "agent_type": "assistant",
        "tools_required": ["file_read", "file_write"],
        "tools_optional": ["bash_executor", "file_list"],
        "tools_requiring_approval": ["bash_executor"],
        "packages": [],
        "memory_namespace": "dev-assistant",
        "llm_backend": "precise",
        "supports_streaming": True,
        "supports_a2a": True,
        "step_budget": {"max_steps": 100, "max_tool_calls": 80, "wall_clock_secs": 1800},
        "tags": ["dev", "implementation", "code", "pipeline", "a2a"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
        "examples": [
            "Implémente la spec user-auth",
            "Explore comment le Tool Registry fonctionne dans ce projet",
            "Implémente la couche API de la spec payment-flow, couche par couche",
            "Qu'est-ce qui a déjà été implémenté dans ce workspace ?",
            "Ajoute une feature de login — crée la TaskSpec d'abord si elle n'existe pas",
        ],
        "limitations": [
            "Refuse de commencer sans TaskSpec validée — crée une spec minimale et demande "
            "confirmation si aucune n'existe",
            "Ne génère jamais de tests automatiquement — déléguer à review-assistant",
            "N'efface jamais de code existant sans confirmation explicite",
            "Délègue à code-worker via A2A pour tout fichier de code — si code-worker est "
            "absent, signale l'indisponibilité plutôt que d'écrire directement",
            "N'ajoute jamais de dépendance non présente dans le projet sans validation explicite",
        ],
        "setup_notes": (
            "Fonctionne de manière autonome. Si un fichier .apollia/tasks/{slug}.md existe, "
            "l'assistant le charge automatiquement. Sans TaskSpec préexistante, il propose d'en "
            "créer une minimale et demande validation avant de commencer l'implémentation. "
            "À partir de la deuxième session sur le même projet, les règles projet sont rechargées "
            "depuis la mémoire sémantique sans relire APOLLIA.md ou Cargo.toml. "
            "Deux modes distincts : implémentation (avec TaskSpec) et exploration (réponse sans code). "
            "Fonctionne mieux en pipeline avec spec-assistant (génère la TaskSpec) et review-assistant "
            "(vérifie l'implémentation), mais chaque assistant est utilisable séparément."
        ),
        "skills": [
            {
                "id": "implement",
                "name": "Implémenter une feature",
                "description": (
                    "Charge ou crée une TaskSpec, puis implémente chaque couche cochée "
                    "[x] en déléguant à code-worker via A2A. Demande validation si "
                    "aucune TaskSpec n'existe."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "request": {
                        "type": "string",
                        "description": "Description de la feature à implémenter ou slug de TaskSpec",
                        "required": True,
                    },
                    "spec_slug": {
                        "type": "string",
                        "description": "Slug de la TaskSpec à utiliser (ex. : user-auth)",
                        "required": False,
                    },
                },
            },
            {
                "id": "explore",
                "name": "Explorer la codebase",
                "description": (
                    "Répond aux questions sur la codebase, les patterns et l'architecture "
                    "depuis la mémoire sémantique ou les fichiers du workspace. "
                    "Ne propose jamais d'implémentation spontanée."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "question": {
                        "type": "string",
                        "description": "Question sur la codebase ou l'architecture",
                        "required": True,
                    },
                },
            },
        ],
    }


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------

class DevAssistant(ConversationalAgent):
    """Assistant d'implémentation Apollia OS — deuxième maillon du pipeline dev.

    Implémente les features décrites dans une TaskSpec en respectant les règles
    projet. Impose un contrat pré-tâche (TaskSpec validée) avant toute ligne de
    code et délègue l'écriture de fichiers à code-worker via A2A.

    Session startup behaviour (first turn):
    1. Run ``DevContextBootstrap`` to discover project context (workspace
       rules, tech stack, architecture, recent files) and persist to memory.
    2. Load the snapshot (instant in session N+1 when HEAD hasn't changed).
    3. Parse workspace rules and cache for subsequent turns.

    Deux modes de fonctionnement :

    - **Implémentation** : charge la TaskSpec, implémente chaque couche cochée
      [x] dans "Couches concernées" via code-worker, met à jour la TaskSpec.
      Si aucune TaskSpec n'existe, en crée une minimale et demande validation.
    - **Exploration** : utilise le LLM pour répondre aux questions sur la
      codebase sans proposer d'implémentation spontanée.

    Le mode est déterminé automatiquement depuis l'intention du message
    utilisateur (marqueurs d'implémentation vs. exploration + présence de ``?``).
    """

    SYSTEM_PROMPT: str = _EXPLORE_SYSTEM_PROMPT_FR.format(
        rules_section="(chargées au démarrage de la session)"
    )
    MAX_TURNS: int = 30
    TEMPERATURE: float = 0.2

    def __init__(self) -> None:
        self._bootstrap = DevContextBootstrap()

    def manifest(self) -> dict[str, Any]:
        """Retourne le manifest AIP de dev-assistant."""
        return manifest()

    async def _run_explore(
        self,
        ctx: Any,
        user_message: str,
        lang: str,
        rules: dict[str, str],
        history: list[dict[str, str]],
    ) -> AIPResult:
        """Traite une requête d'exploration via le LLM.

        Construit le system prompt avec les règles projet, appelle le LLM en
        mode conversationnel, et mémorise la réponse pour les sessions futures.
        """
        if ctx.llm is None:
            return AIPResult.failed(
                "NO_LLM",
                "DevAssistant requires ctx.llm for exploration mode",
            )

        self.SYSTEM_PROMPT = build_explore_system_prompt(lang, rules)
        response_text, _ = await self.converse(
            ctx, user_message, history=history or None
        )

        if ctx.memory is not None:
            key = f"{MEMORY_KEY_CODEBASE_PREFIX}{_slugify(user_message[:40])}"
            await ctx.memory.remember(
                key=key,
                value=response_text[:1000],
                source=_MEMORY_SOURCE,
                confidence=_MEMORY_CONFIDENCE_CODEBASE,
            )

        return AIPResult.completed(response_text)

    async def _run_implement(
        self,
        ctx: Any,
        user_message: str,
        lang: str,
        rules: dict[str, str],
        history: list[dict[str, str]],
    ) -> AIPResult:
        """Traite une requête d'implémentation couche par couche.

        Cherche une TaskSpec dans le message ou le workspace. Si aucune n'est
        trouvée, crée une spec minimale et retourne ``input_required`` pour
        demander validation. Si une spec est trouvée, délègue chaque couche
        cochée à code-worker via A2A.
        """
        spec_slug = _extract_spec_slug(user_message)
        spec_content: str | None = None

        if spec_slug:
            spec_content = await load_task_spec(ctx, spec_slug)

        if spec_content is None:
            slug = _slugify(user_message[:50]) or "new-task"
            minimal = create_minimal_task_spec(slug, user_message, rules)
            await write_task_spec(ctx, slug, minimal)

            if lang == "fr":
                prompt = (
                    f"J'ai créé une TaskSpec minimale dans "
                    f"`.apollia/tasks/{slug}.md`.\n\n"
                    f"Veuillez :\n"
                    f"1. Ouvrir le fichier et cocher `[x]` les couches à implémenter\n"
                    f"2. Préciser l'objectif si nécessaire\n"
                    f"3. Me confirmer avec : "
                    f"\"commence l'implémentation de la spec {slug}\""
                )
            else:
                prompt = (
                    f"I've created a minimal TaskSpec at "
                    f"`.apollia/tasks/{slug}.md`.\n\n"
                    f"Please:\n"
                    f"1. Open the file and check `[x]` the layers to implement\n"
                    f"2. Clarify the objective if needed\n"
                    f"3. Confirm with: \"start implementing spec {slug}\""
                )
            return AIPResult.input_required(prompt)

        layers = extract_layers_to_implement(spec_content)
        objective = extract_objective(spec_content)
        resolved_slug = spec_slug or _slugify(user_message[:50])

        if not layers:
            msg = (
                "Toutes les couches de la TaskSpec sont déjà implémentées (✅) "
                "ou aucune couche n'est cochée [x]."
                if lang == "fr"
                else "All TaskSpec layers are already implemented (✅) "
                "or no layer is checked [x]."
            )
            return AIPResult.completed(msg)

        output_parts: list[str] = []

        for layer in layers:
            header = (
                f"## Implémentation — {layer}"
                if lang == "fr"
                else f"## Implementing — {layer}"
            )
            output_parts.append(header)

            layer_output = await delegate_to_code_worker(
                ctx, layer, objective, spec_content, rules
            )
            output_parts.append(layer_output)

            spec_content = mark_layer_implemented(spec_content, layer)
            await write_task_spec(ctx, resolved_slug, spec_content)

        completion = (
            "\nImplémentation terminée. Voulez-vous que je crée un commit ?"
            if lang == "fr"
            else "\nImplementation complete. Would you like me to create a commit?"
        )
        output_parts.append(completion)

        if ctx.memory is not None:
            await ctx.memory.record(
                content=(
                    f"Implemented: {objective[:200]}\n"
                    f"Layers: {', '.join(layers)}"
                ),
                importance=0.7,
            )

        return AIPResult.completed("\n\n".join(output_parts))

    async def run(self, task: Any, ctx: Any) -> dict[str, Any]:
        """Exécute un tour de conversation pour *task*.

        Extrait l'entrée utilisateur, détecte la langue et l'intention,
        bootstrappe le contexte projet au premier tour, puis route vers le
        mode exploration ou implémentation.
        """
        input_text, history = _extract_task_input(task)

        if not input_text:
            return AIPResult.failed("NO_INPUT", "No input provided in task")

        lang = _detect_language(input_text)
        is_first_turn = not history

        if is_first_turn:
            self._current_lang: str = lang

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
            self._rules: dict[str, str] = (
                _parse_rules(raw_rules)
                if raw_rules
                else {"raw": "", "forbidden_deps": "[]", "patterns": "", "comment_convention": ""}
            )
        else:
            lang = getattr(self, "_current_lang", lang)

        rules = getattr(
            self, "_rules",
            {"raw": "", "forbidden_deps": "[]", "patterns": "", "comment_convention": ""},
        )
        intent = _detect_intent(input_text)

        if intent == "explore":
            return await self._run_explore(ctx, input_text, lang, rules, history)

        return await self._run_implement(ctx, input_text, lang, rules, history)


# ---------------------------------------------------------------------------
# Module-level agent instance (contrat AIP)
# ---------------------------------------------------------------------------

agent = DevAssistant()

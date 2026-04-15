"""document-assistant — Generalist document processing assistant for Apollia OS.

Routes natural-language requests about files (Excel, CSV, PDF, SQLite) to
specialized workers via A2A and returns results in plain business language.
No technical jargon is ever exposed to the user.

Position in the architecture::

    User (any profile)
      └── document-assistant
            ├── A2A → excel-worker      (.xlsx, .xlsm, .xls)
            ├── A2A → csv-data-worker   (.csv, .tsv)
            ├── A2A → pdf-worker        (.pdf)
            └── A2A → sql-worker        (.sqlite, .sqlite3, .db)

Tools required  : file_read
Tools optional  : bash_executor
A2A             : excel-worker, csv-data-worker, pdf-worker, sql-worker
"""

from __future__ import annotations

import json
import re
import time
from pathlib import Path
from typing import Any

from apollia.agents import AIPResult, ConversationalAgent
from apollia.bootstrap import ContextBootstrap


# ---------------------------------------------------------------------------
# Routing tables
# ---------------------------------------------------------------------------

ROUTING_TABLE: dict[str, str] = {
    ".xlsx": "excel-worker",
    ".xlsm": "excel-worker",
    ".xls": "excel-worker",
    ".csv": "csv-data-worker",
    ".tsv": "csv-data-worker",
    ".pdf": "pdf-worker",
    ".sqlite": "sql-worker",
    ".sqlite3": "sql-worker",
    ".db": "sql-worker",
}

# Each entry: (keywords, worker). Checked in order; first match wins.
_KEYWORD_ROUTING: tuple[tuple[tuple[str, ...], str], ...] = (
    (("excel", "tableur", "feuille", "spreadsheet", "workbook"), "excel-worker"),
    (("csv", "séparateur", "virgule", "tsv"), "csv-data-worker"),
    (("pdf", "rapport", "document"), "pdf-worker"),
    (("sql", "database", "query", "requête", "sqlite"), "sql-worker"),
)


# ---------------------------------------------------------------------------
# Memory keys
# ---------------------------------------------------------------------------

MEMORY_KEY_FORMAT_PREF: str = "user_format_pref"
MEMORY_KEY_RECENT_FILES: str = "recent_files"

_MEMORY_SOURCE: str = "document-assistant"
_MEMORY_CONFIDENCE: float = 0.8
_MAX_RECENT_FILES: int = 10


# ---------------------------------------------------------------------------
# Bootstrap
# ---------------------------------------------------------------------------


class DocumentContextBootstrap(ContextBootstrap):
    """Bootstrap for document-assistant.

    Discovers the user's document processing profile: format preferences,
    recently analysed files, and available A2A workers.  Unlike the
    development-domain agents, staleness is timestamp-based (7 days max)
    because document-assistant does not operate on a git repository.

    Snapshot structure::

        {
            "last_scan_ts": str,            # epoch timestamp (staleness marker)
            "format_preferences": dict,     # persisted output format preferences
            "recent_files": list[str],      # recently analysed file paths
            "available_workers": list[str]  # A2A workers active at last bootstrap
        }

    Legacy memory keys ``user_format_pref`` and ``recent_files`` are read
    during bootstrap and migrated into the snapshot.  The original keys are
    kept in memory for backward compatibility.
    """

    _MAX_AGE_SECS: int = 7 * 24 * 3600  # 7 days

    async def is_stale(self, ctx: Any) -> bool:
        """Return ``True`` when the snapshot is older than 7 days."""
        meta = await self.load_meta(ctx)
        if meta is None:
            return True
        ts = meta.get("staleness_marker", "")
        try:
            last = float(ts)
        except (ValueError, TypeError):
            return True
        return (time.time() - last) > self._MAX_AGE_SECS

    async def run_bootstrap(self, ctx: Any) -> dict[str, Any]:
        """Discover document processing profile and persist the snapshot.

        Reads legacy memory keys (``user_format_pref``, ``recent_files``),
        lists available A2A workers, and persists everything as a snapshot.
        Legacy keys are intentionally not deleted for backward compatibility.
        """
        snapshot: dict[str, Any] = {}
        now_ts = str(time.time())

        # Format preferences (legacy key migration)
        fmt_raw = await ctx.memory.recall(MEMORY_KEY_FORMAT_PREF) if ctx.memory else None
        snapshot["format_preferences"] = json.loads(fmt_raw) if fmt_raw else {}

        # Recent files (legacy key migration)
        recent_raw = await ctx.memory.recall(MEMORY_KEY_RECENT_FILES) if ctx.memory else None
        snapshot["recent_files"] = json.loads(recent_raw) if recent_raw else []

        # Available A2A workers
        workers: list[str] = []
        try:
            skills = await ctx.a2a_list_skills()
            workers = sorted({s["agent_name"] for s in skills})
        except Exception:
            pass
        snapshot["available_workers"] = workers

        await self.persist(
            ctx, snapshot, staleness_marker=now_ts, source="bootstrap:document"
        )
        return snapshot


# ---------------------------------------------------------------------------
# Language detection
# ---------------------------------------------------------------------------

_FRENCH_MARKERS: frozenset[str] = frozenset((
    "bonjour", "salut", "bonsoir",
    "je", "nous", "vous", "il", "elle", "ils",
    "est", "sont", "avec", "pour", "dans", "sur", "par",
    "une", "un", "le", "la", "les", "des", "du",
    "mon", "ton", "son", "notre", "de",
    "quel", "quelle", "quels", "quelles",
    "résume", "analyse", "montre", "donne", "calcule", "compare",
    "comment", "pourquoi", "quoi", "oui", "non", "merci",
    "fichier", "colonne", "données", "total", "rapport",
))


def _detect_language(text: str) -> str:
    """Return ``"fr"`` when *text* is likely French, ``"en"`` otherwise.

    Uses whole-token matching to avoid false positives from common
    substrings (e.g. ``"en"`` inside ``"document"``).
    """
    tokens = set(re.split(r"\W+", text.lower()))
    hits = sum(1 for marker in _FRENCH_MARKERS if marker in tokens)
    return "fr" if hits >= 2 else "en"


# ---------------------------------------------------------------------------
# File path extraction
# ---------------------------------------------------------------------------

# Matches bare filenames and paths with supported document extensions.
_FILE_PATH_RE: re.Pattern[str] = re.compile(
    r"""['""]?(?:[\w./\\-]+/)?[\w.-]+\.(?:xlsx|xlsm|xls|csv|tsv|pdf|sqlite|sqlite3|db)['""]?""",
    re.IGNORECASE,
)


def _extract_file_paths(text: str) -> list[str]:
    """Extract all document file paths mentioned in *text*.

    Returns a deduplicated list in order of first appearance, stripping
    surrounding quotes when present.
    """
    seen: set[str] = set()
    paths: list[str] = []
    for raw in _FILE_PATH_RE.findall(text):
        path = raw.strip("'\"")
        if path not in seen:
            paths.append(path)
            seen.add(path)
    return paths


# ---------------------------------------------------------------------------
# Routing helpers
# ---------------------------------------------------------------------------

def _route_by_extension(file_path: str) -> str | None:
    """Return the worker name for *file_path* based on its extension.

    Returns ``None`` when the extension is not in :data:`ROUTING_TABLE`.
    """
    suffix = Path(file_path).suffix.lower()
    return ROUTING_TABLE.get(suffix)


def _route_by_keywords(text: str) -> str | None:
    """Return the worker name suggested by keywords in *text*.

    Scans :data:`_KEYWORD_ROUTING` in order and returns the first match.
    Returns ``None`` when no keyword matches.
    """
    lower = text.lower()
    for keywords, worker in _KEYWORD_ROUTING:
        if any(kw in lower for kw in keywords):
            return worker
    return None


# ---------------------------------------------------------------------------
# Instruction building
# ---------------------------------------------------------------------------

def _build_worker_instruction(user_request: str, file_path: str, lang: str) -> str:
    """Build a precise, structured instruction for the target worker.

    The instruction includes the file path and the original user request
    so the worker has full context without needing to parse conversation history.
    """
    if lang == "fr":
        return (
            f"Fichier : {file_path}\n"
            f"Demande : {user_request}\n"
            f"Réponds en français avec les données extraites du fichier."
        )
    return (
        f"File: {file_path}\n"
        f"Request: {user_request}\n"
        f"Reply in English with the data extracted from the file."
    )


# ---------------------------------------------------------------------------
# Error humanization
# ---------------------------------------------------------------------------

def _humanize_error(error: Exception, file_path: str, lang: str) -> str:
    """Translate a technical worker exception into a user-readable message.

    Maps common error patterns to friendly, actionable explanations.
    No stack trace or class name is ever included in the output.
    """
    error_str = str(error).lower()

    if lang == "fr":
        if "column" in error_str or "colonne" in error_str:
            return (
                f"La colonne demandée n'existe pas dans **{file_path}**. "
                f"Précisez le nom exact de la colonne (la casse est importante)."
            )
        if "no such file" in error_str or "not found" in error_str:
            return (
                f"Je n'ai pas trouvé le fichier **{file_path}**. "
                f"Vérifiez que le chemin est correct et que le fichier est dans votre espace de travail."
            )
        if "corrupt" in error_str or "invalid" in error_str:
            return (
                f"Le fichier **{file_path}** semble corrompu ou dans un format non supporté. "
                f"Essayez de l'ouvrir dans son application native pour vérifier son intégrité."
            )
        if "permission" in error_str or "access" in error_str:
            return (
                f"Accès refusé au fichier **{file_path}**. "
                f"Vérifiez que le fichier n'est pas ouvert dans une autre application."
            )
        return (
            f"Une erreur s'est produite lors du traitement de **{file_path}**. "
            f"Réessayez ou vérifiez que le fichier est valide."
        )

    if "column" in error_str:
        return (
            f"The requested column does not exist in **{file_path}**. "
            f"Specify the exact column name (case-sensitive)."
        )
    if "no such file" in error_str or "not found" in error_str:
        return (
            f"File **{file_path}** not found. "
            f"Check that the path is correct and the file is in your workspace."
        )
    if "corrupt" in error_str or "invalid" in error_str:
        return (
            f"File **{file_path}** appears corrupted or in an unsupported format. "
            f"Try opening it in its native application to verify its integrity."
        )
    if "permission" in error_str or "access" in error_str:
        return (
            f"Access denied to **{file_path}**. "
            f"Check that the file is not open in another application."
        )
    return (
        f"An error occurred while processing **{file_path}**. "
        f"Please retry or verify the file is valid."
    )


# ---------------------------------------------------------------------------
# Response formatting
# ---------------------------------------------------------------------------

def _format_worker_result(result: dict[str, Any], file_path: str, lang: str) -> str:
    """Extract the worker output and return it as a clean string.

    Handles different output shapes (``output``, ``result``, ``text`` keys)
    and list values. Falls back to a neutral message when the result is empty.
    """
    output: Any = result.get("output") or result.get("result") or result.get("text", "")
    if isinstance(output, list):
        output = "\n".join(str(item) for item in output)
    text = str(output).strip()
    if not text:
        if lang == "fr":
            return f"Le worker a traité **{file_path}** mais n'a retourné aucun résultat."
        return f"The worker processed **{file_path}** but returned no result."
    return text


def _format_combined_response(
    results: list[tuple[str, str]],
    lang: str,
) -> str:
    """Combine multiple worker outputs into a single cohesive response.

    *results* is an ordered list of ``(file_path, formatted_output)`` pairs.
    Single-file results are returned as-is without wrapping.
    """
    if len(results) == 1:
        return results[0][1]

    parts: list[str] = []
    for file_path, output in results:
        parts.append(f"**{file_path}**\n\n{output}")

    header = (
        "Voici les résultats pour chaque fichier :\n\n"
        if lang == "fr"
        else "Here are the results for each file:\n\n"
    )
    return header + "\n\n---\n\n".join(parts)


# ---------------------------------------------------------------------------
# Memory helpers
# ---------------------------------------------------------------------------

async def _load_recent_files(ctx: Any) -> list[str]:
    """Load the list of recently analysed files from semantic memory."""
    if ctx.memory is None:
        return []
    raw = await ctx.memory.recall(MEMORY_KEY_RECENT_FILES)
    if not raw:
        return []
    try:
        parsed = json.loads(raw)
        return parsed if isinstance(parsed, list) else []
    except (json.JSONDecodeError, TypeError):
        return []


async def _save_recent_files(ctx: Any, new_files: list[str]) -> None:
    """Merge *new_files* into the persisted recent files list.

    Deduplicates and caps the list at :data:`_MAX_RECENT_FILES` entries,
    keeping the most recently accessed files first.
    """
    if ctx.memory is None:
        return
    current = await _load_recent_files(ctx)
    merged: list[str] = []
    seen: set[str] = set()
    for path in new_files + current:
        if path not in seen:
            merged.append(path)
            seen.add(path)
    await ctx.memory.remember(
        key=MEMORY_KEY_RECENT_FILES,
        value=json.dumps(merged[:_MAX_RECENT_FILES]),
        source=_MEMORY_SOURCE,
        confidence=_MEMORY_CONFIDENCE,
    )


async def _load_format_pref(ctx: Any) -> str | None:
    """Return the persisted output format preference, or ``None`` if absent."""
    if ctx.memory is None:
        return None
    return await ctx.memory.recall(MEMORY_KEY_FORMAT_PREF)


async def _save_format_pref(ctx: Any, pref: str) -> None:
    """Persist the detected or requested output format preference."""
    if ctx.memory is None:
        return
    await ctx.memory.remember(
        key=MEMORY_KEY_FORMAT_PREF,
        value=pref,
        source=_MEMORY_SOURCE,
        confidence=_MEMORY_CONFIDENCE,
    )


# ---------------------------------------------------------------------------
# Format preference detection
# ---------------------------------------------------------------------------

_FORMAT_PREF_RE: re.Pattern[str] = re.compile(
    r"\b(tableau|table|bullet|liste|prose|résumé|summary)\b",
    re.IGNORECASE,
)

_FORMAT_PREF_MAP: dict[str, str] = {
    "tableau": "table",
    "table": "table",
    "bullet": "bullet",
    "liste": "bullet",
    "prose": "prose",
    "résumé": "summary",
    "summary": "summary",
}


def _detect_format_pref(text: str) -> str | None:
    """Detect an explicit output format preference in *text*.

    Returns a normalised format name (``"table"``, ``"bullet"``,
    ``"prose"``, ``"summary"``), or ``None`` when not detected.
    """
    match = _FORMAT_PREF_RE.search(text)
    if match:
        return _FORMAT_PREF_MAP.get(match.group(1).lower())
    return None


# ---------------------------------------------------------------------------
# Task input extraction
# ---------------------------------------------------------------------------

def _extract_task_input(task: Any) -> tuple[str, list[dict[str, str]]]:
    """Extract the user message text and conversation history from *task*.

    Supports both dict-style (test / API format) and attribute-style
    (PyO3 runtime format) task objects.
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
# System prompts
# ---------------------------------------------------------------------------

_SYSTEM_PROMPT_FR: str = """\
Tu es **document-assistant**, l'assistant généraliste d'Apollia OS pour le traitement \
de documents.

Tu traites les fichiers Excel, CSV, PDF et les bases de données SQLite en déléguant \
à des workers spécialisés. Tu restitues les résultats en langage métier clair, \
sans aucun jargon technique.

## Contraintes absolues

1. Tu ne lis JAMAIS directement un fichier — tu délègues toujours au worker approprié.
2. Tu poses au maximum UNE question de clarification si la demande est ambiguë.
3. Tu ne mentionnes jamais pandas, openpyxl, pdfplumber ni aucun détail d'implémentation.
4. Toutes les données restent sur la machine de l'utilisateur.
5. Tu réponds dans la même langue que l'utilisateur (FR/EN auto-détecté).\
"""

_SYSTEM_PROMPT_EN: str = """\
You are **document-assistant**, Apollia OS's generalist document processing assistant.

You process Excel, CSV, PDF files and SQLite databases by delegating to specialized \
workers. You present results in clear business language with no technical jargon.

## Absolute constraints

1. You NEVER read a file directly — you always delegate to the appropriate worker.
2. You ask at most ONE clarifying question when the request is ambiguous.
3. You never mention pandas, openpyxl, pdfplumber or any implementation detail.
4. All data stays on the user's machine.
5. You respond in the same language as the user (FR/EN auto-detected).\
"""


# ---------------------------------------------------------------------------
# Module-level manifest function (AIP contract)
# ---------------------------------------------------------------------------

def manifest() -> dict[str, Any]:
    """Return the AIP manifest of document-assistant."""
    return {
        "name": "document-assistant",
        "version": "1.0.0",
        "description": (
            "Assistant de traitement de documents : analyse Excel, CSV, PDF et bases de données "
            "en langage naturel. Délègue aux workers spécialisés via A2A. "
            "Conçu pour tous les profils — aucune compétence technique requise. "
            "Répond en langage métier clair, sans jargon Python ou SQL."
        ),
        "execution_mode": "auto",
        "agent_type": "assistant",
        "tools_required": ["file_read"],
        "tools_optional": ["bash_executor"],
        "packages": [],
        "memory_namespace": "document-assistant",
        "supports_streaming": True,
        "supports_a2a": True,
        "step_budget": {"max_steps": 50, "max_tool_calls": 40, "wall_clock_secs": 600},
        "tags": ["documents", "data", "excel", "csv", "pdf", "sql", "généraliste", "métier"],
        "max_concurrent_tasks": 3,
        "dangerous_tools_allowed": False,
        "examples": [
            "Analyse ce fichier Excel et donne-moi les totaux par catégorie",
            "Résume ce rapport PDF en 5 points clés",
            "Compare les colonnes Revenu et Dépenses du fichier ventes.csv",
            "Quelles sont les 10 commandes les plus rentables dans ma base clients.sqlite ?",
            "Quelles sont les 5 catégories qui ont le plus progressé ce trimestre dans ventes-2026.xlsx ?",
        ],
        "limitations": [
            "Ne lit jamais les fichiers directement — délègue toujours aux workers spécialisés via A2A",
            "Requiert qu'au moins un worker document soit installé et actif",
            "La taille maximale des fichiers est limitée par la configuration des workers sous-jacents",
            "Ne présente jamais de code Python, de traces d'erreur ou de jargon technique — traduit "
            "toujours les erreurs workers en message métier compréhensible",
            "Ne suppose jamais l'emplacement d'un fichier — demande le chemin si non précisé",
        ],
        "setup_notes": (
            "Conçu pour les profils non-développeurs (comptable, analyste, juriste, chef de projet) "
            "qui travaillent avec des fichiers sans connaître le fonctionnement technique des workers. "
            "Nécessite au moins un worker document installé et actif : excel-worker (.xlsx/.xls), "
            "csv-data-worker (.csv/.tsv), pdf-worker (.pdf) ou sql-worker (.sqlite/.db). "
            "Sans worker actif, l'assistant signale clairement l'indisponibilité et indique lequel "
            "installer. Chaque worker peut être installé séparément selon les formats utilisés. "
            "Mémorise les préférences de format de l'utilisateur (tableau, liste, synthèse) "
            "pour les sessions suivantes."
        ),
        "skills": [
            {
                "id": "analyze",
                "name": "Analyser un document",
                "description": (
                    "Analyse un fichier (Excel, CSV, PDF, SQLite) en réponse à une demande "
                    "en langage naturel. Retourne les résultats en langage métier."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "request": {
                        "type": "string",
                        "description": "Question ou demande en langage naturel sur le fichier",
                        "required": True,
                    },
                    "file": {
                        "type": "string",
                        "description": "Chemin vers le fichier à analyser",
                        "required": False,
                    },
                },
            },
        ],
    }


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------

class DocumentAssistant(ConversationalAgent):
    """Generalist document processing assistant for Apollia OS.

    Identifies the appropriate specialized worker from the file extension in
    the user request, delegates the analysis via A2A, and returns the results
    in clear business language — without exposing any technical implementation
    detail to the user.

    Uses ``DocumentContextBootstrap`` to persist the user's document
    processing profile (format preferences, recent files, available A2A
    workers) in semantic memory.  On session N+1, the snapshot is reloaded
    without re-scanning.

    Workflow per ``run()`` call:

    1. Bootstrap document profile (first turn) or reload snapshot.
    2. Extract the user message and detect the language (FR/EN).
    3. Find document file paths in the message and map each to a worker.
    4. When no file path is identified, ask a single clarifying question.
    5. Call each worker sequentially via ``ctx.a2a.call()``.
    6. Humanize any worker errors; combine multi-file results into one response.
    7. Persist analysed file paths in semantic memory for session continuity.
    """

    SYSTEM_PROMPT: str = _SYSTEM_PROMPT_FR
    MAX_TURNS: int = 20
    TEMPERATURE: float = 0.3

    def __init__(self) -> None:
        self._bootstrap = DocumentContextBootstrap()

    def manifest(self) -> dict[str, Any]:
        """Return the AIP manifest of document-assistant."""
        return manifest()

    async def run(self, task: Any, ctx: Any) -> dict[str, Any]:
        """Process one document request turn.

        Bootstraps document profile on the first turn, detects language,
        routes to the right worker(s), delegates via A2A, and returns
        results formatted as natural-language output.
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
        else:
            lang = getattr(self, "_current_lang", lang)

        self.SYSTEM_PROMPT = _SYSTEM_PROMPT_FR if lang == "fr" else _SYSTEM_PROMPT_EN

        detected_pref = _detect_format_pref(input_text)
        if detected_pref:
            current_pref = await _load_format_pref(ctx)
            if detected_pref != current_pref:
                await _save_format_pref(ctx, detected_pref)

        file_paths = _extract_file_paths(input_text)
        routing_pairs: list[tuple[str, str]] = []
        for fp in file_paths:
            worker = _route_by_extension(fp)
            if worker:
                routing_pairs.append((fp, worker))

        if not routing_pairs:
            if lang == "fr":
                prompt = "De quel fichier souhaitez-vous que je m'occupe ?"
            else:
                prompt = "Which file would you like me to work with?"
            return AIPResult.input_required(prompt)

        if not (hasattr(ctx, "a2a") and ctx.a2a is not None):
            if lang == "fr":
                fallback = (
                    "Le traitement A2A n'est pas disponible dans ce contexte. "
                    "Démarrez Apollia OS pour accéder aux workers de documents."
                )
            else:
                fallback = (
                    "A2A processing is not available in this context. "
                    "Start Apollia OS to access document workers."
                )
            return AIPResult.completed(fallback)

        collected: list[tuple[str, str]] = []
        errors: list[str] = []

        for file_path, worker in routing_pairs:
            instruction = _build_worker_instruction(input_text, file_path, lang)
            try:
                result = await ctx.a2a.call(
                    agent=worker,
                    skill="analyze",
                    input={"task": instruction, "file": file_path},
                )
                collected.append((file_path, _format_worker_result(result, file_path, lang)))
            except Exception as exc:
                errors.append(_humanize_error(exc, file_path, lang))

        await _save_recent_files(ctx, [fp for fp, _ in routing_pairs])

        if not collected and errors:
            return AIPResult.completed("\n\n".join(errors))

        output_parts: list[str] = []
        if collected:
            output_parts.append(_format_combined_response(collected, lang))
        if errors:
            error_header = "**Problèmes rencontrés :**" if lang == "fr" else "**Issues encountered:**"
            error_lines = "\n".join(f"- {e}" for e in errors)
            output_parts.append(f"{error_header}\n{error_lines}")

        return AIPResult.completed("\n\n".join(output_parts))


# ---------------------------------------------------------------------------
# Module-level agent instance (AIP contract)
# ---------------------------------------------------------------------------

agent = DocumentAssistant()

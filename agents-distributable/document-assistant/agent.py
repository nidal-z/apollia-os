"""document-assistant — Consultant agent for document processing.

Routes natural-language requests about files (Excel, CSV, PDF, SQLite, …) to
specialized workers via A2A and returns results in clear business language —
no technical jargon exposed to the user.

Position in the architecture::

    User (any profile)
      └── document-assistant
            ├── A2A → excel-worker      (read-excel, analyze-excel, edit-excel)
            ├── A2A → csv-data-worker   (read-csv, analyze-csv, transform-csv)
            ├── A2A → pdf-worker        (read-pdf, extract-text, extract-tables)
            └── A2A → sql-worker        (query-sql, schema-inspect, data-export)

Fonctionnement
--------------
Agent ReAct agnostique. Le LLM lit le message, détermine le fichier concerné,
choisit le bon outil A2A (``a2a:analyze-excel``, ``a2a:read-pdf``, etc.), et
reformule le résultat en langage métier. Si le chemin du fichier manque, il
appelle ``ask_user`` plutôt que de deviner.

Outils requis : file_read, ask_user
Outils optionnels : bash_executor, file_list, + tous les skills a2a:*
"""

from __future__ import annotations

from typing import Any

from apollia.agents import AIPResult, BaseReActAgent
from apollia.utils.hitl import resume_pending_tool


# ---------------------------------------------------------------------------
# Available workers discovery (called fresh at each turn, no persistence)
# ---------------------------------------------------------------------------


async def _discover_available_workers(ctx: Any) -> list[str]:
    """Return the list of active A2A agent names, or ``[]`` on failure.

    The runtime exposes ``ctx.a2a_list_skills`` when A2A routing is wired.
    Called once per chat turn to populate the system prompt — cheap
    enough that persistence adds no value.
    """
    fn = getattr(ctx, "a2a_list_skills", None)
    if fn is None:
        return []
    try:
        skills = await fn()
    except Exception:
        return []
    if not skills:
        return []
    return sorted({s["agent_name"] for s in skills if "agent_name" in s})


# ---------------------------------------------------------------------------
# Error humanization (kept — used when an A2A call fails)
# ---------------------------------------------------------------------------


def _humanize_error(error: Exception | str, file_path: str) -> str:
    """Translate a technical worker exception into a user-readable message.

    Maps common error patterns to friendly, actionable explanations.
    No stack trace or class name is ever included in the output.
    """
    error_str = str(error).lower()

    if "column" in error_str:
        return (
            f"The requested column does not exist in **{file_path}**. "
            "Specify the exact column name (case-sensitive)."
        )
    if "no such file" in error_str or "not found" in error_str:
        return (
            f"File **{file_path}** not found. "
            "Check that the path is correct and the file is in your workspace."
        )
    if "corrupt" in error_str or "invalid" in error_str:
        return (
            f"File **{file_path}** appears corrupted or in an unsupported "
            "format. Try opening it in its native application to verify its "
            "integrity."
        )
    if "permission" in error_str or "access" in error_str:
        return (
            f"Access denied to **{file_path}**. "
            "Check that the file is not open in another application."
        )
    return (
        f"An error occurred while processing **{file_path}**. "
        "Please retry or verify the file is valid."
    )


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------

_SYSTEM_PROMPT_TEMPLATE: str = """\
You are **document-assistant**, Apollia OS's generalist document processing \
assistant.

You help users extract, analyse, and understand documents — Excel workbooks, \
CSV files, PDFs, SQLite databases — by delegating the actual heavy lifting \
to specialised workers via A2A. You yourself never open or parse files; you \
pick the right worker and translate the result into plain business language.

## Absolute constraints

1. **You never read or parse document files directly.** Always delegate to \
   an A2A worker. `file_read` is for small text companion files only (notes, \
   schemas), never for `.xlsx`/`.csv`/`.pdf`/`.sqlite`.
2. **No technical jargon.** Never mention pandas, openpyxl, pdfplumber, \
   SQL internals, stack traces, class names. Translate everything into \
   plain user-facing language.
3. **All data stays on the user's machine** — don't propose uploading files \
   anywhere.

## Routing guide (which A2A skill for which file)

- `.xlsx`, `.xlsm`, `.xls` → `a2a:read-excel`, `a2a:analyze-excel`, or \
  `a2a:edit-excel` depending on intent.
- `.csv`, `.tsv` → `a2a:read-csv`, `a2a:analyze-csv`, or \
  `a2a:transform-csv`.
- `.pdf` → `a2a:read-pdf`, `a2a:extract-text`, or `a2a:extract-tables`.
- `.sqlite`, `.sqlite3`, `.db` → `a2a:query-sql`, `a2a:schema-inspect`, \
  or `a2a:data-export`.

For other formats: check `available_workers` below, or ask the user how \
they'd like to proceed.

## Workflow

1. **Read the user's message.** Identify the file path(s) and the intent \
   (read, analyse, extract, transform, edit).
2. **If the file path is missing or ambiguous**, call `ask_user` with one \
   precise question. Do not guess.
3. **Pick the right A2A skill** from the routing guide. If the intent is \
   analytical ("give me totals", "top 10", "summarise"), prefer the \
   `analyze-*` / `extract-*` skills over raw `read-*`.
4. **Call the A2A tool** with a clear task payload: include the file path \
   and the user's original request. Example payload: \
   `{{"task": "Sum the Revenue column grouped by Region.", "file": \
   "ventes.xlsx"}}`.
5. **Translate the worker's output** into clear business language. Strip \
   technical jargon. If the user has a format preference (see below), \
   respect it (table, bullet list, prose, summary).
6. **On failure**, translate the error into a friendly message \
   (file not found, column not in file, corrupted file…) — never expose \
   the raw exception.

## What you never do

- Never parse an `.xlsx`/`.csv`/`.pdf`/`.sqlite` file yourself.
- Never return code, SQL queries, or pandas expressions.
- Never mention the implementation worker names to the user (they care \
  about the *result*, not the *how*).

## Available A2A workers (detected at session start)

{workers_section}

## Language

Always respond in the same language as the user's message.
"""


def _build_system_prompt(available_workers: list[str]) -> str:
    """Compose the document-assistant system prompt.

    The list of available workers is the only piece of external context
    injected — format preferences or recent file lists are history-driven
    (visible in the conversation) so they don't need to be pre-loaded.
    """
    if available_workers:
        workers_str = "\n".join(f"- `{w}`" for w in sorted(available_workers))
        workers_section = (
            f"Workers available right now in this runtime:\n{workers_str}\n\n"
            "If the user needs a format whose worker is not in this list, "
            "tell them which worker to install."
        )
    else:
        workers_section = (
            "No workers detected yet — the available list may become "
            "populated later. If an `a2a:*` tool call fails with a "
            "'no active agent' error, tell the user which worker to "
            "install (excel-worker, csv-data-worker, pdf-worker, sql-worker)."
        )

    return _SYSTEM_PROMPT_TEMPLATE.format(workers_section=workers_section)


# ---------------------------------------------------------------------------
# Task input extraction
# ---------------------------------------------------------------------------

def _extract_task_input(task: Any) -> tuple[str, list[dict[str, str]]]:
    """Extract the user message text and conversation history from *task*."""
    task_input = (
        task.get("input") if isinstance(task, dict)
        else getattr(task, "input", None)
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
# Manifest
# ---------------------------------------------------------------------------

def manifest() -> dict[str, Any]:
    """Return the AIP manifest of document-assistant."""
    return {
        "name": "document-assistant",
        "version": "2.0.0",
        "description": (
            "Consultant agent for document processing — Excel, CSV, PDF, "
            "SQLite. Identifies the right specialised worker via A2A and "
            "translates results into plain business language. Designed for "
            "any profile — no technical skill required. Stack-agnostic: "
            "asks the user when the format is unknown."
        ),
        "execution_mode": "auto",
        "agent_type": "assistant",
        "tools_required": ["file_read", "ask_user"],
        "tools_optional": [
            "bash_executor",
            "file_list",
            # Excel
            "a2a:read-excel",
            "a2a:analyze-excel",
            "a2a:edit-excel",
            # CSV
            "a2a:read-csv",
            "a2a:analyze-csv",
            "a2a:transform-csv",
            # PDF
            "a2a:read-pdf",
            "a2a:extract-text",
            "a2a:extract-tables",
            # SQL
            "a2a:query-sql",
            "a2a:schema-inspect",
            "a2a:data-export",
        ],
        "tools_requiring_approval": [],
        "packages": [],
        "memory_namespace": "document-assistant",
        "llm_backend": "precise",
        "supports_streaming": True,
        "supports_a2a": True,
        "step_budget": {
            "max_steps": 40,
            "max_tool_calls": 30,
            "wall_clock_secs": 600,
        },
        "tags": [
            "documents", "data", "excel", "csv", "pdf", "sql",
            "généraliste", "métier", "consultant", "a2a-orchestrator",
        ],
        "max_concurrent_tasks": 3,
        "dangerous_tools_allowed": False,
        "examples": [
            "Analyse ce fichier Excel et donne-moi les totaux par catégorie",
            "Résume ce rapport PDF en 5 points clés",
            "Compare les colonnes Revenu et Dépenses du fichier ventes.csv",
            "Quelles sont les 10 commandes les plus rentables dans ma base clients.sqlite ?",
        ],
        "limitations": [
            "Ne lit jamais les fichiers documents directement — délègue "
            "toujours aux workers spécialisés via A2A.",
            "Requiert qu'au moins un worker document soit installé et actif "
            "pour traiter le format demandé.",
            "Ne présente jamais de code ou de jargon technique — traduit les "
            "erreurs workers en message métier.",
        ],
        "setup_notes": (
            "Conçu pour les profils non-développeurs (comptable, analyste, "
            "juriste, chef de projet) qui travaillent avec des fichiers "
            "sans connaître leur fonctionnement technique. Nécessite au "
            "moins un worker document installé et actif : excel-worker, "
            "csv-data-worker, pdf-worker ou sql-worker. Sans worker actif "
            "pour le format demandé, l'assistant signale clairement "
            "l'indisponibilité et indique lequel installer."
        ),
        "skills": [
            {
                "id": "analyze-document",
                "name": "Analyser un document",
                "description": (
                    "Analyse un fichier (Excel, CSV, PDF, SQLite) en "
                    "réponse à une demande en langage naturel. Retourne les "
                    "résultats en langage métier."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "request": {
                        "type": "string",
                        "description": (
                            "Question ou demande en langage naturel sur le "
                            "fichier"
                        ),
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


class DocumentAssistant(BaseReActAgent):
    """Consultant agent for document processing.

    Routes requests to specialised workers via A2A and surfaces results in
    plain business language. No regex routing, no hard-coded extension map —
    the LLM picks the right skill from the routing guide in the system
    prompt.
    """

    SYSTEM_PROMPT: str = _SYSTEM_PROMPT_TEMPLATE.format(
        workers_section="(loaded per-turn)",
    )
    MAX_STEPS: int = 20
    TEMPERATURE: float = 0.3

    def manifest(self) -> dict[str, Any]:
        """Return the AIP manifest of document-assistant."""
        return manifest()

    async def run(self, task: Any, ctx: Any) -> dict[str, Any]:
        """Execute one conversational turn via the ReAct loop."""
        if ctx.llm is None:
            return AIPResult.failed(
                "NO_LLM",
                "DocumentAssistant requires ctx.llm — no LLM backend configured",
            )

        input_text, history = _extract_task_input(task)
        if not input_text:
            return AIPResult.failed("NO_INPUT", "No input provided in task")

        available_workers = await _discover_available_workers(ctx)
        self.SYSTEM_PROMPT = _build_system_prompt(available_workers)

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

agent = DocumentAssistant()

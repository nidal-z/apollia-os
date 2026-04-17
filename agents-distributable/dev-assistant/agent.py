"""dev-assistant — Consultant agent for implementation and codebase exploration.

Second link of the spec → dev → review pipeline. Loads a TaskSpec, plans
layer-by-layer, and delegates actual file writes to ``code-worker`` via A2A.
Also answers exploration questions about the codebase.

Fonctionnement
--------------
Agent ReAct agnostique. Le LLM décide au fil de la conversation :
- répondre à une question d'exploration (lire des fichiers, chercher dans la
  mémoire sémantique, rédiger une explication)
- charger une TaskSpec et déléguer sa mise en œuvre à ``a2a:generate-code``,
  ``a2a:refactor-code``, ``a2a:review-code``, ou créer un commit via
  ``a2a:git-commit``
- créer une TaskSpec minimale si aucune n'existe et demander validation

Aucun regex de détection d'intent, aucun marqueur d'implémentation. Le LLM
choisit selon le message.

Outils requis : file_read, file_write
Outils optionnels : bash_executor, file_list, file_grep, ask_user,
  memory_search, a2a:generate-code, a2a:refactor-code, a2a:review-code,
  a2a:git-commit
"""

from __future__ import annotations

from typing import Any

from apollia.agents import AIPResult, BaseReActAgent
from apollia.utils.hitl import resume_pending_tool

from lib import workspace_rules


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------

_SYSTEM_PROMPT_TEMPLATE: str = """\
You are **dev-assistant**, a senior developer assisting on implementation and \
codebase questions, regardless of tech stack.

Your role shifts naturally between two modes depending on what the user asks:

1. **Exploration** — answer questions about the code, the architecture, \
   patterns, conventions. Read files with `file_read`, grep with `file_grep`, \
   list the tree with `file_list`, recall prior context with `memory_search`. \
   Give grounded answers based on what you actually saw in the project.

2. **Implementation** — carry a feature from its TaskSpec to code. Load the \
   spec with `file_read`, verify the scope, then delegate the actual code \
   edits to specialised agents via A2A:
   - `a2a:generate-code` — create new source files
   - `a2a:refactor-code` — modify existing files
   - `a2a:review-code` — check an implementation against conventions
   - `a2a:git-commit` — stage + commit the result
   Update the TaskSpec between layers to track progress.

You choose the mode based on the user's intent. A question ending with "?" \
usually means exploration; "implement X" or "finish the spec Y" means \
implementation. When in doubt, ask with `ask_user`.

## Core behaviour

**Discover before acting.** You haven't scanned the project yet. On a \
non-trivial request, start by listing what's there (`file_list` on the \
root, `file_glob` for specific patterns) or looking at recent activity \
(`bash_executor git log --oneline -n 10`). Never assume a stack.

**Contract before implementation.** Don't start coding without a TaskSpec. \
If the user asks to implement something and no spec exists, either:
- propose a minimal TaskSpec and ask for validation, or
- delegate back to `spec-assistant` via A2A if the need is significantly \
  underspecified.

**Respect project rules.** If the workspace defines forbidden dependencies, \
mandatory patterns, or code conventions (see rules below), enforce them when \
delegating to `code-worker` by including them in the A2A payload.

**Clean code by default.** No comments that repeat the function name. No \
dependencies added without validation. No code deleted without confirmation. \
Only comments that explain non-obvious logic.

## What you never do

- Never write code yourself — delegate to `a2a:generate-code` / \
  `a2a:refactor-code`. You orchestrate.
- Never skip the TaskSpec. If missing, create a minimal one first.
- Never invent project context (stack, rules, conventions) that isn't in the \
  loaded context or the user's message.

## Project context

{rules_section}

## Language

Always respond in the same language as the user's message.
"""


def _build_system_prompt(raw_rules: str) -> str:
    """Compose the dev-assistant system prompt from the workspace rules."""
    rules = (raw_rules or "").strip()
    if rules:
        truncated = rules[:4000]
        if len(rules) > 4000:
            truncated += "\n[... truncated for context window ...]"
        rules_section = (
            "**Workspace rules loaded** (project-specific conventions — "
            "authoritative for this project):\n"
            f"```\n{truncated}\n```"
        )
    else:
        rules_section = (
            "No rules file found in this workspace. Ask the user about "
            "conventions and constraints before implementing anything."
        )

    return _SYSTEM_PROMPT_TEMPLATE.format(rules_section=rules_section)


# ---------------------------------------------------------------------------
# Task input extraction
# ---------------------------------------------------------------------------

def _extract_task_input(task: Any) -> tuple[str, list[dict[str, str]]]:
    """Extract user text + conversation history from *task* (chat mode)."""
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
    """Return the AIP agent manifest for dev-assistant."""
    return {
        "name": "dev-assistant",
        "version": "2.0.0",
        "description": (
            "Consultant agent for codebase exploration and feature "
            "implementation. Loads a TaskSpec, plans layer by layer, "
            "delegates actual code writes to code-worker via A2A. Also "
            "answers architecture and implementation questions by reading "
            "the project. Stack-agnostic: adapts to Rust, Python, JS, or "
            "any other language present in the workspace."
        ),
        "execution_mode": "auto",
        "agent_type": "assistant",
        "tools_required": ["file_read", "file_write"],
        "tools_optional": [
            "bash_executor",
            "file_list",
            "file_grep",
            "ask_user",
            "memory_search",
            "a2a:generate-code",
            "a2a:refactor-code",
            "a2a:review-code",
            "a2a:git-commit",
        ],
        "tools_requiring_approval": ["bash_executor"],
        "packages": [],
        "memory_namespace": "dev-assistant",
        "llm_backend": "precise",
        "supports_streaming": True,
        "supports_a2a": True,
        "step_budget": {
            "max_steps": 40,
            "max_tool_calls": 30,
            "wall_clock_secs": 900,
        },
        "tags": [
            "development", "implementation", "exploration",
            "pipeline-dev", "consultant", "agnostic", "a2a-orchestrator",
        ],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
        "examples": [
            "Comment fonctionne le système d'authentification ?",
            "Implémente la spec user-auth",
            "Montre-moi les endpoints API existants",
            "Refactore le module payment pour extraire la validation",
        ],
        "limitations": [
            "N'écrit pas le code lui-même — délègue à code-worker via A2A.",
            "Ne démarre pas une implémentation sans TaskSpec validée.",
        ],
        "setup_notes": (
            "Fonctionne avec ou sans règles projet. En implémentation, "
            "requiert que code-worker et git-worker soient installés et "
            "actifs pour que les A2A fonctionnent. Sinon, bascule en mode "
            "exploration et propose des plans textuels au lieu d'éditer "
            "des fichiers."
        ),
        "skills": [
            {
                "id": "explore-codebase",
                "name": "Explorer le code et l'architecture",
                "description": (
                    "Répond à des questions sur le codebase : architecture, "
                    "conventions, patterns, dépendances. Lit les fichiers "
                    "pertinents via file_read et file_grep."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "question": {
                        "type": "string",
                        "description": "Question sur le code ou l'architecture",
                        "required": True,
                    },
                },
            },
            {
                "id": "implement-spec",
                "name": "Implémenter une TaskSpec",
                "description": (
                    "Charge une TaskSpec existante, planifie son "
                    "implémentation couche par couche, délègue chaque "
                    "couche à code-worker via A2A, met à jour la spec au "
                    "fur et à mesure."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "spec_slug": {
                        "type": "string",
                        "description": "Slug de la TaskSpec à implémenter",
                        "required": True,
                    },
                },
            },
        ],
    }


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------

class DevAssistant(BaseReActAgent):
    """Consultant agent for implementation and codebase exploration.

    Behaviour :
    - Charge un snapshot cross-session (règles, architecture, fichiers récents).
    - Délègue au LLM toute décision : exploration vs implémentation, quels
      fichiers lire, quand appeler code-worker, quand mettre à jour la spec.
    - Préserve l'historique conversationnel multi-tour en chat mode.
    """

    SYSTEM_PROMPT: str = _SYSTEM_PROMPT_TEMPLATE.format(
        rules_section="(loaded per-turn)",
    )
    MAX_STEPS: int = 30
    TEMPERATURE: float = 0.2

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for dev-assistant."""
        return manifest()

    async def run(self, task: Any, ctx: Any) -> dict[str, Any]:
        """Execute one conversational turn via the ReAct loop."""
        if ctx.llm is None:
            return AIPResult.failed(
                "NO_LLM",
                "DevAssistant requires ctx.llm — no LLM backend configured",
            )

        input_text, history = _extract_task_input(task)
        if not input_text:
            return AIPResult.failed("NO_INPUT", "No input provided in task")

        self.SYSTEM_PROMPT = _build_system_prompt(workspace_rules(ctx))

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


agent = DevAssistant()

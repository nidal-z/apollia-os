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

try:
    from shared.project_bootstrap import ProjectContextBootstrap
except ModuleNotFoundError:
    from assistants.shared.project_bootstrap import ProjectContextBootstrap


# ---------------------------------------------------------------------------
# Context Bootstrap (cross-session cache)
# ---------------------------------------------------------------------------


class DevContextBootstrap(ProjectContextBootstrap):
    """Bootstrap for dev-assistant.

    Extends the common project snapshot with:

    - ``architecture``: modules / package / service markers found in the tree
    - ``recent_files``: files modified in the last ~10 commits
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

        try:
            arch_result = await ctx.tools.call("bash_executor", {
                "command": (
                    "find . -maxdepth 3 \\( "
                    "-name 'Cargo.toml' -o -name 'mod.rs' "
                    "-o -name '__init__.py' -o -name 'index.ts' "
                    "\\) 2>/dev/null | grep -v target | head -40 | sort"
                ),
                "timeout_secs": 10,
            })
        except Exception:
            arch_result = None
        if arch_result and arch_result.get("stdout"):
            raw = arch_result["stdout"]
            if isinstance(raw, str):
                modules = [line.strip() for line in raw.split("\n") if line.strip()]

        try:
            recent_result = await ctx.tools.call("bash_executor", {
                "command": (
                    "git diff --name-only HEAD~10 HEAD 2>/dev/null "
                    "|| find . -maxdepth 3 -newer .git/HEAD 2>/dev/null | head -30"
                ),
                "timeout_secs": 10,
            })
        except Exception:
            recent_result = None
        if recent_result and recent_result.get("stdout"):
            raw = recent_result["stdout"]
            if isinstance(raw, str):
                recent = [line.strip() for line in raw.split("\n") if line.strip()]

        return {"architecture": modules, "recent_files": recent}


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------

_SYSTEM_PROMPT_TEMPLATE: str = """\
You are **dev-assistant**, a senior developer assisting on implementation and \
codebase questions, regardless of tech stack.

Your role shifts naturally between two modes depending on what the user asks:

1. **Exploration** — answer questions about the code, the architecture, \
   patterns, conventions. Read files with `file_read`, grep with `file_grep`, \
   recall prior context with `memory_search`. Give grounded answers based on \
   what you actually saw in the project.

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

**Understand before acting.** Project context (rules, architecture hints, \
recent files) is loaded below. Use it. If essential info is missing, call \
`ask_user` with adapted questions — never use generic templates.

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

## Architecture hints

{architecture_section}

## Recent files

{recent_files_section}

## Language

Always respond in the same language as the user's message.
"""


def _build_system_prompt(
    raw_rules: str,
    architecture: list[str],
    recent_files: list[str],
) -> str:
    """Compose the dev-assistant system prompt from the cached snapshot."""
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

    if architecture:
        arch_lines = "\n".join(f"- `{m}`" for m in architecture[:30])
        architecture_section = (
            f"Module/package markers detected in the workspace:\n{arch_lines}"
        )
    else:
        architecture_section = (
            "No obvious module markers detected (no Cargo.toml, package.json, "
            "__init__.py, index.ts in the top 3 levels)."
        )

    if recent_files:
        recent_lines = "\n".join(f"- `{f}`" for f in recent_files[:20])
        recent_files_section = (
            "Files modified in the last ~10 commits "
            f"(useful hints about active areas):\n{recent_lines}"
        )
    else:
        recent_files_section = "No recent file changes detected."

    return _SYSTEM_PROMPT_TEMPLATE.format(
        rules_section=rules_section,
        architecture_section=architecture_section,
        recent_files_section=recent_files_section,
    )


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
        architecture_section="(loaded per-turn)",
        recent_files_section="(loaded per-turn)",
    )
    MAX_STEPS: int = 30
    TEMPERATURE: float = 0.2

    def __init__(self) -> None:
        self._bootstrap = DevContextBootstrap()

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

        if await self._bootstrap.needs_bootstrap(ctx):
            await self._bootstrap.run_bootstrap(ctx)
        snapshot = await self._bootstrap.load_snapshot(ctx) or {}

        raw_rules = snapshot.get("workspace_rules", "")
        if not raw_rules:
            ws = getattr(ctx, "workspace", None)
            raw_rules = (ws.rules or "") if ws is not None else ""

        architecture: list[str] = snapshot.get("architecture", []) or []
        recent_files: list[str] = snapshot.get("recent_files", []) or []

        self.SYSTEM_PROMPT = _build_system_prompt(
            raw_rules, architecture, recent_files
        )

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

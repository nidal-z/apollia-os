"""review-assistant — Post-implementation reviewer agent for Apollia OS.

Third and final link of the development pipeline (spec → dev → review).
Reviews an implementation against its TaskSpec and the project's own
conventions.

Fonctionnement
--------------
Agent ReAct agnostique. Aucun regex Rust, TypeScript ou Python hard-coded,
aucune détection de test runner en dur. Le LLM découvre le contexte du
projet (lit les manifests, comprend la stack), lit la TaskSpec, récupère le
diff, inspecte les modifications, lance les tests adaptés, et compose un
rapport structuré 🟢 LGTM / 🟡 ATTENTION / 🔴 BLOQUANT.

Le ton et les critères viennent du workspace : si un ``APOLLIA.md`` (ou
équivalent) liste les dépendances interdites, les patterns obligatoires, ou
les conventions de commentaires, le LLM les applique. Sinon, il se limite
aux vérifications génériques (le diff fait-il ce que la spec demande ? les
tests passent-ils ?).

Outils requis : file_read, bash_executor
Outils optionnels : file_write, file_edit, file_list, file_grep, ask_user,
  memory_search
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
# Bootstrap
# ---------------------------------------------------------------------------


class ReviewContextBootstrap(ProjectContextBootstrap):
    """Bootstrap for review-assistant.

    No extra scopes beyond the common project snapshot (workspace rules,
    tech stack markers, git state). The base snapshot is sufficient —
    the LLM decides which additional checks to run via its tools.
    """


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------

_SYSTEM_PROMPT_TEMPLATE: str = """\
You are **review-assistant**, a senior code reviewer acting as an IT \
consultant on this project.

Your role: verify that a recent implementation matches its TaskSpec, \
respects the project's own conventions, and passes the test suite. You \
produce a structured, actionable review report. You work on any stack — \
Rust, Python, TypeScript, Go, Ruby, Elixir, or any mix — because you \
*discover* the stack before judging it.

## Core methodology

**Discover before criticising.** You don't assume what "good code" looks \
like in a project you haven't inspected. Before reviewing:
1. Read the workspace rules below. If they're loaded, they define what this \
   project considers BLOCKING vs. ATTENTION vs. LGTM.
2. Identify the tech stack by reading manifest files (`Cargo.toml`, \
   `package.json`, `pyproject.toml`, `go.mod`, `Gemfile`, `mix.exs`, etc.) \
   via `file_read`. Tech-stack hints from the bootstrap are below.
3. Read the TaskSpec being implemented (usually at \
   `.apollia/tasks/{{slug}}.md`) to know the expected scope and layers.
4. Retrieve the diff via `bash_executor` — typically \
   `git diff HEAD`, fall back to `git diff HEAD~1` if HEAD is clean.
5. Run the relevant tests via `bash_executor` with the command appropriate \
   to the stack (`cargo test`, `pytest`, `npm test`, `go test`, …). If the \
   workspace rules specify a test command, use that.

**Never hallucinate checks.** If you haven't read the file, you don't know \
what's in it. If you haven't run the tests, you don't know if they pass. \
Every 🔴 BLOQUANT / 🟡 ATTENTION / 🟢 LGTM claim must be backed by a tool \
call you actually made in this conversation.

**Ask before assuming.** If the TaskSpec is missing, the diff is empty, or \
the stack is unclear — call `ask_user` with one precise question. Don't \
fabricate a review.

## What you check

1. **Layer completeness.** Does the diff actually touch each layer the \
   TaskSpec marked as in-scope? Layers mentioned in the spec but absent \
   from the diff → 🔴 BLOQUANT. Spec-silent changes in the diff → 🟡 \
   ATTENTION (possibly scope creep).

2. **Conformity to project rules.** If the workspace rules explicitly list \
   forbidden dependencies, mandatory patterns, or comment conventions, \
   check for violations in the diff. Use `file_grep` or `bash_executor` \
   with targeted greps. Violations → 🔴 BLOQUANT. Style drifts not in the \
   rules → 🟡 ATTENTION at most.

3. **Test coverage & pass.** Run the tests with the right command for the \
   stack. Failures → 🔴 BLOQUANT. Passing tests + no new test for a new \
   public function → 🟡 ATTENTION. Full suite passes → 🟢 LGTM.

## Report format

End your turn with a final_answer containing a markdown report structured \
like this:

```
## Review — <spec-title> — <YYYY-MM-DD>

### Layer completeness
🟢 / 🟡 / 🔴 <item> — <one-line reason, with file:line reference when useful>
…

### Standards conformity
🟢 / 🟡 / 🔴 <item> — <reason>
…

### Tests
🟢 / 🟡 / 🔴 <item> — <reason>
…

### Summary
🔴 <N> blocking item(s) to fix before merge
🟡 <N> attention item(s) (non-blocking)
🟢 <conclusion>
```

Use 🟢 "Ready to merge" when there are zero 🔴. Use 🔴 to block only on \
violations you can point to — spec expectation, rule text, or failing test.

## What you never do

- Never apply a fix automatically — this is a review, not a rewrite.
- Never run destructive bash commands (`rm -rf`, `git push --force`, …).
- Never invent rules that aren't in the workspace file. If something looks \
  stylistically off but isn't codified, flag it as 🟡 ATTENTION, not 🔴.

## Project context

{rules_section}

## Tech stack hints

{tech_stack_section}

## Language

Always respond in the same language as the user's message.
"""


def _build_system_prompt(
    raw_rules: str,
    tech_stack: list[str],
) -> str:
    """Compose the review-assistant system prompt."""
    rules = (raw_rules or "").strip()
    if rules:
        truncated = rules[:4000]
        if len(rules) > 4000:
            truncated += "\n[... truncated for context window ...]"
        rules_section = (
            "**Workspace rules loaded** (project-specific conventions — "
            "authoritative for BLOQUANT vs. ATTENTION decisions in this "
            "project):\n"
            f"```\n{truncated}\n```"
        )
    else:
        rules_section = (
            "No rules file found in this workspace. "
            "Limit yourself to generic checks: does the diff match the "
            "TaskSpec, do the tests pass. Don't enforce conventions that "
            "aren't written down — ask the user if unsure."
        )

    if tech_stack:
        hints = ", ".join(f"`{m}`" for m in tech_stack)
        tech_stack_section = (
            f"Manifest files detected in the workspace: {hints}. "
            "Use them to infer the stack and pick the right test command. "
            "Read them if you need more detail."
        )
    else:
        tech_stack_section = (
            "No standard manifest files detected in the top 3 levels. "
            "Inspect the tree via `file_list` or `file_glob` to identify "
            "the stack before running any tests."
        )

    return _SYSTEM_PROMPT_TEMPLATE.format(
        rules_section=rules_section,
        tech_stack_section=tech_stack_section,
    )


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
    """Return the AIP agent manifest for review-assistant."""
    return {
        "name": "review-assistant",
        "version": "2.0.0",
        "description": (
            "Consultant-style post-implementation reviewer. Reads the "
            "TaskSpec, retrieves the diff, runs the stack-appropriate "
            "tests, and produces a structured 🟢/🟡/🔴 report. "
            "Stack-agnostic: discovers the project's tech stack and rules "
            "before judging — never applies hard-coded language-specific "
            "heuristics."
        ),
        "execution_mode": "auto",
        "agent_type": "assistant",
        "tools_required": ["file_read", "bash_executor"],
        "tools_optional": [
            "file_write",
            "file_edit",
            "file_list",
            "file_grep",
            "ask_user",
            "memory_search",
        ],
        "tools_requiring_approval": ["bash_executor"],
        "packages": [],
        "memory_namespace": "review-assistant",
        "llm_backend": "precise",
        "supports_streaming": True,
        "supports_a2a": True,
        "step_budget": {
            "max_steps": 30,
            "max_tool_calls": 25,
            "wall_clock_secs": 900,
        },
        "tags": [
            "review", "verification", "pipeline-dev",
            "consultant", "agnostic",
        ],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
        "examples": [
            "Review la spec user-auth",
            "Vérifie que l'implémentation couvre bien le diff actuel",
            "Relance les tests et dis-moi s'ils passent",
        ],
        "limitations": [
            "N'applique jamais de fix automatiquement — c'est une revue, "
            "pas une réécriture.",
            "Requiert un accès au diff git pour être utile — utilise "
            "bash_executor.",
        ],
        "setup_notes": (
            "Fonctionne sur n'importe quelle stack. Sans fichier de règles "
            "projet (`APOLLIA.md`), se limite aux vérifications génériques "
            "(spec vs diff, tests passants). Avec fichier de règles, "
            "applique les contraintes qui y sont listées (dépendances "
            "interdites, patterns obligatoires, conventions de commentaires)."
        ),
        "skills": [
            {
                "id": "review-implementation",
                "name": "Reviewer une implémentation",
                "description": (
                    "Charge la TaskSpec, récupère le diff, lance les tests "
                    "adaptés à la stack, et produit un rapport structuré "
                    "🟢 LGTM / 🟡 ATTENTION / 🔴 BLOQUANT."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "spec_slug": {
                        "type": "string",
                        "description": "Slug de la TaskSpec à reviewer",
                        "required": False,
                    },
                    "focus": {
                        "type": "string",
                        "description": (
                            "Aspect particulier à examiner "
                            "(sécurité, performance, idiomes)"
                        ),
                        "required": False,
                    },
                },
            },
        ],
    }


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------


class ReviewAssistant(BaseReActAgent):
    """Consultant-style code reviewer — last link of spec→dev→review.

    Discovers the project's stack and rules first, then reviews the diff
    against the TaskSpec. No hard-coded checks: everything is driven by
    tools and the workspace's own documented conventions.
    """

    SYSTEM_PROMPT: str = _SYSTEM_PROMPT_TEMPLATE.format(
        rules_section="(loaded per-turn)",
        tech_stack_section="(loaded per-turn)",
    )
    MAX_STEPS: int = 25
    TEMPERATURE: float = 0.2

    def __init__(self) -> None:
        self._bootstrap = ReviewContextBootstrap()

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for review-assistant."""
        return manifest()

    async def run(self, task: Any, ctx: Any) -> dict[str, Any]:
        """Execute one conversational turn via the ReAct loop."""
        if ctx.llm is None:
            return AIPResult.failed(
                "NO_LLM",
                "ReviewAssistant requires ctx.llm — no LLM backend configured",
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

        tech_stack: list[str] = snapshot.get("tech_stack", []) or []

        self.SYSTEM_PROMPT = _build_system_prompt(raw_rules, tech_stack)

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

agent = ReviewAssistant()

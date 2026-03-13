"""debug_orchestrator — Agent de debug pour le bloc 10 (ORIA Mode Orchestré).

Implémente le pattern orchestré directement dans run() pour permettre un test
end-to-end sans LLM mock :

    Phase 1 — Planification : le LLM génère un plan JSON à 3 étapes.
              Fallback : plan statique prédéfini si le LLM échoue ou est absent.

    Phase 2 — Exécution séquentielle : chaque step est exécuté via bash_executor
              en respectant les dépendances (s1 → s2 → s3).

    Phase 3 — Synthèse : le LLM produit un résumé structuré des outputs.
              Fallback : concaténation brute des outputs si LLM absent.

Déploiement :
    apollia-os agent start agents/debug_orchestrator.py
    apollia-os agent info debug-orchestrator

Exécution manuelle :
    apollia-os run debug-orchestrator "Compte les crates et les tests dans $(pwd)"

Exécution via trigger cron (apollia.toml) :
    Déclenché toutes les minutes avec une tâche d'audit du workspace.
    Observer avec : apollia-os trigger logs debug-cron-bloc10

Flux observable complet :
    → Agent starts
    → Phase 1: LLM plan generated (3 steps) / static fallback
    → Phase 2: Step s1 executed (bash) — output logged
    → Phase 2: Step s2 executed (bash) — output logged
    → Phase 2: Step s3 executed (bash) — output logged
    → Phase 3: LLM synthesis
    → Markdown report returned
"""

import asyncio
import json
import re
import subprocess


# ─────────────────────────────────────────────────────────────────────────────
# Constants
# ─────────────────────────────────────────────────────────────────────────────

# Commandes bash hardcodées — utilisées si la génération LLM échoue ou est absente.
# Chaque step est une commande bash autonome qui produit une sortie textuelle.
FALLBACK_PLAN = [
    {
        "step_id": "s1",
        "description": "ls -1 {workdir}/crates/ | sort",
        "tool_hint": "bash_executor",
        "depends_on": [],
    },
    {
        "step_id": "s2",
        "description": (
            "find {workdir}/crates -name '*.rs' -not -path '*/target/*' | wc -l"
        ),
        "tool_hint": "bash_executor",
        "depends_on": ["s1"],
    },
    {
        "step_id": "s3",
        "description": "git -C {workdir} log --oneline -5",
        "tool_hint": "bash_executor",
        "depends_on": ["s2"],
    },
]

# Prompt envoyé au LLM pour générer le plan JSON.
# Très directif pour maximiser les chances d'un bon JSON avec un petit modèle.
PLANNER_SYSTEM_PROMPT = """\
You are a bash command planner. Generate a 3-step execution plan as valid JSON.

Rules:
- Each "description" MUST be a valid bash command (never natural language).
- Use only: ls, find, wc, git, grep, cat, echo, head, tail.
- "tool_hint" must be "bash_executor" for bash steps.
- "depends_on" must only reference step_ids defined earlier in the same plan.
- Return ONLY valid JSON, no markdown fences, no explanation.

Output format (exact):
{"steps": [
  {"step_id": "s1", "description": "<bash_command>", "tool_hint": "bash_executor", "depends_on": []},
  {"step_id": "s2", "description": "<bash_command>", "tool_hint": "bash_executor", "depends_on": ["s1"]},
  {"step_id": "s3", "description": "<bash_command>", "tool_hint": "bash_executor", "depends_on": ["s2"]}
]}"""

# Longueur max d'un output de step avant troncature dans le rapport.
MAX_OUTPUT_CHARS = 800

# Timeout bash (secondes) pour chaque step.
BASH_TIMEOUT_SECONDS = 15


# ─────────────────────────────────────────────────────────────────────────────
# JSON plan parsing
# ─────────────────────────────────────────────────────────────────────────────

def _parse_plan_json(raw: str) -> list:
    """Extract the steps list from a raw LLM response.

    Tries three strategies:
      1. Direct JSON parse of the full text.
      2. Extract the first {...} block found in the text.
      3. Return empty list on all failures.
    """
    text = raw.strip()

    # Strip Markdown fences if present.
    text = re.sub(r"```(?:json)?\s*", "", text)
    text = text.rstrip("`").strip()

    # Strategy 1: full JSON parse.
    try:
        data = json.loads(text)
        if isinstance(data, dict) and isinstance(data.get("steps"), list):
            return data["steps"]
    except (json.JSONDecodeError, ValueError):
        pass

    # Strategy 2: find outermost braces.
    start, end = text.find("{"), text.rfind("}")
    if start != -1 and end > start:
        try:
            data = json.loads(text[start:end + 1])
            if isinstance(data, dict) and isinstance(data.get("steps"), list):
                return data["steps"]
        except (json.JSONDecodeError, ValueError):
            pass

    return []


# ─────────────────────────────────────────────────────────────────────────────
# Agent
# ─────────────────────────────────────────────────────────────────────────────

class DebugOrchestrator:
    """Debug agent for ORIA orchestrated mode (bloc 10).

    Manifest fields:
        name:             debug-orchestrator
        version:          1.0.0
        tools_required:   bash_executor
        memory_namespace: debug-orchestrator
        execution_mode:   orchestrated

    Run phases:
        Phase 1 — Plan: LLM generates a 3-step JSON plan, or fallback is used.
        Phase 2 — Execute: each step run via bash_executor in dependency order.
        Phase 3 — Synthesize: LLM produces a summary, or raw concat is used.
    """

    def manifest(self):
        return {
            "name": "debug-orchestrator",
            "version": "1.0.0",
            "description": (
                "Debug agent for ORIA bloc 10 (Mode Orchestré). "
                "Implements plan → execute → synthesize pattern end-to-end in run(). "
                "Designed for cron-triggered workspace audits."
            ),
            "tools_required": ["bash_executor"],
            "memory_namespace": "debug-orchestrator",
            "execution_mode": "orchestrated",
            # system_prompt requis par ORIAEngine.execute_orchestrated_plan().
            # Utilisé quand l'agent est dispatché via l'engine ORIA natif.
            "system_prompt": (
                "Tu es un agent d'audit de codebase Rust. "
                "Pour chaque tâche d'audit, génère un plan de 3 étapes bash concrètes "
                "puis exécute-les séquentiellement pour produire un rapport structuré."
            ),
        }

    async def run(self, task, ctx):
        """Main entry point — implements the orchestrated loop in Python.

        This method covers the production path where AIPProductionBackend
        calls run() directly (bloc 10 debug via the full runtime stack).

        The ORIA Rust engine (ORIAEngine::execute_orchestrated_plan) covers
        the same pattern at the Rust level and is tested via integration tests.

        Args:
            task: AIP task dict — input parts carry the audit directive.
            ctx:  RuntimeContext with ctx.llm, ctx.tools, ctx.memory.
        """
        # ── 1. Parse input ────────────────────────────────────────────────
        parts = task["input"].get("parts", [])
        user_input = (
            parts[0].get("text", "Audit le workspace Apollia").strip()
            if parts
            else "Audit le workspace Apollia"
        )

        # Extraire le workdir depuis le premier token qui ressemble à un chemin.
        workdir = _extract_workdir(user_input)

        # ── 2. Phase 1 — Plan generation ─────────────────────────────────
        plan_steps, plan_source = await self._generate_plan(ctx, user_input, workdir)

        # ── 3. Phase 2 — Sequential execution ────────────────────────────
        step_outputs = {}   # {step_id: output_str}
        step_log = []       # [(step_id, description, output)]

        for step in plan_steps:
            step_id = step.get("step_id", "?")
            raw_desc = step.get("description", "echo 'no-op'")
            tool_hint = step.get("tool_hint", "bash_executor")

            # Interpolate previous step outputs referenced as {s1}, {s2}, etc.
            description = _interpolate(raw_desc, step_outputs)
            description = description.replace("{workdir}", workdir)

            if tool_hint == "bash_executor":
                output = await self._run_bash(ctx, description)
            elif tool_hint == "llm" and ctx.llm is not None:
                context_str = "\n".join(
                    f"{k}:\n{v[:400]}" for k, v in step_outputs.items()
                )
                output = await self._run_llm_step(ctx, description, context_str)
            else:
                # tool_hint unsupported or LLM absent → static passthrough.
                output = f"[skipped — tool '{tool_hint}' unavailable]"

            step_outputs[step_id] = output
            step_log.append((step_id, description, output))

        # ── 4. Phase 3 — Synthesis ────────────────────────────────────────
        synthesis = await self._synthesize(ctx, user_input, step_log)

        # ── 5. Memory ─────────────────────────────────────────────────────
        if ctx.memory is not None:
            try:
                await ctx.memory.record(
                    f"debug-orchestrator: {user_input[:60]}",
                    importance=0.4,
                    task_id=task["task_id"],
                )
            except Exception:
                pass  # Memory errors must never crash the agent.

        # ── 6. Return report ──────────────────────────────────────────────
        report = _build_report(user_input, plan_source, step_log, synthesis)
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": report}],
        }

    # ── Phase 1: plan generation ──────────────────────────────────────────

    async def _generate_plan(self, ctx, user_input: str, workdir: str):
        """Call the LLM to generate a 3-step bash plan, or use the fallback plan.

        Returns (steps_list, source_tag) where source_tag is one of:
            "llm-generated"    — LLM returned a valid plan.
            "static-fallback"  — LLM unavailable or returned invalid JSON.
        """
        if ctx.llm is None:
            return _apply_workdir(FALLBACK_PLAN, workdir), "static-fallback (no LLM)"

        user_prompt = (
            f"Generate a 3-step bash audit plan for this task:\n\n{user_input}\n\n"
            f"Working directory: {workdir}\n\n"
            f"All commands must work in: {workdir}"
        )
        try:
            response = await ctx.llm.chat(
                system=PLANNER_SYSTEM_PROMPT,
                user=user_prompt,
            )
            steps = _parse_plan_json(response.content)
            if steps:
                # Validate: each step must have step_id and description.
                if all(s.get("step_id") and s.get("description") for s in steps):
                    return steps, "llm-generated"
        except Exception:
            pass

        # Fallback when LLM returned invalid JSON or an exception occurred.
        return _apply_workdir(FALLBACK_PLAN, workdir), "static-fallback (LLM invalid JSON)"

    # ── Phase 2 helpers ───────────────────────────────────────────────────

    async def _run_bash(self, ctx, command: str) -> str:
        """Run a bash command via ctx.tools, falling back to stdlib subprocess."""
        if ctx.tools is not None:
            try:
                result = await ctx.tools.call(
                    "bash_executor",
                    {"command": command, "timeout_seconds": BASH_TIMEOUT_SECONDS},
                )
                stdout = result.get("stdout", "")
                stderr = result.get("stderr", "")
                return stdout if stdout else stderr
            except Exception as exc:
                return f"[tools error: {exc}]"

        # Fallback: stdlib subprocess (no tool proxy needed for debug).
        loop = asyncio.get_event_loop()
        try:
            proc = await loop.run_in_executor(
                None,
                lambda: subprocess.run(
                    command,
                    shell=True,
                    capture_output=True,
                    text=True,
                    timeout=BASH_TIMEOUT_SECONDS,
                ),
            )
            return proc.stdout if proc.stdout.strip() else proc.stderr
        except Exception as exc:
            return f"[subprocess error: {exc}]"

    async def _run_llm_step(self, ctx, description: str, context_str: str) -> str:
        """Call the LLM for a synthesis step."""
        try:
            response = await ctx.llm.chat(
                system="You are a concise technical analyst. Answer in 2-3 sentences.",
                user=f"Context:\n{context_str}\n\nTask: {description}",
            )
            return response.content
        except Exception as exc:
            return f"[LLM step error: {exc}]"

    # ── Phase 3: synthesis ────────────────────────────────────────────────

    async def _synthesize(self, ctx, task: str, step_log: list) -> str:
        """Produce a brief synthesis of all step outputs via the LLM.

        Falls back to static concatenation when LLM is absent.
        """
        if ctx.llm is None:
            return "_(LLM not available — see raw step outputs above)_"

        summary_parts = [
            f"Step {sid} (`{desc[:60]}`):\n{out[:400]}"
            for sid, desc, out in step_log
        ]
        summary = "\n\n".join(summary_parts)

        try:
            response = await ctx.llm.chat(
                system=(
                    "You are a technical analyst. "
                    "Produce a brief 3-sentence synthesis of the audit results below."
                ),
                user=f"Audit task: {task}\n\nResults:\n{summary}",
            )
            return response.content
        except Exception as exc:
            return f"_(Synthesis failed: {exc})_"


# ─────────────────────────────────────────────────────────────────────────────
# Pure helpers (no side effects — unit-testable)
# ─────────────────────────────────────────────────────────────────────────────

def _extract_workdir(text: str) -> str:
    """Extract the first absolute path from a text string.

    Falls back to the current working directory if none is found.
    Used to derive the working directory from the task input text.
    """
    match = re.search(r"(/[^\s]+)", text)
    if match:
        candidate = match.group(1)
        # Keep only the directory part (drop filenames).
        import os
        if os.path.isdir(candidate):
            return candidate
        parent = os.path.dirname(candidate)
        if os.path.isdir(parent):
            return parent
    import os
    return os.getcwd()


def _apply_workdir(plan: list, workdir: str) -> list:
    """Return a copy of the fallback plan with {workdir} substituted."""
    return [
        {**step, "description": step["description"].replace("{workdir}", workdir)}
        for step in plan
    ]


def _interpolate(text: str, outputs: dict) -> str:
    """Replace {step_id} tokens in text with the corresponding step output.

    Example: "Analyse {s1} and compare with {s2}" →
             "Analyse <output_of_s1> and compare with <output_of_s2>"
    """
    for step_id, output in outputs.items():
        text = text.replace(f"{{{step_id}}}", output[:200])
    return text


def _build_report(
    task: str,
    plan_source: str,
    step_log: list,
    synthesis: str,
) -> str:
    """Build the Markdown debug report from all execution artifacts."""
    steps_md = "\n\n".join(
        f"### Step {sid}\n\n"
        f"**Command:** `{desc}`\n\n"
        f"**Output:**\n```\n{out[:MAX_OUTPUT_CHARS].strip()}\n"
        + ("... [truncated]\n" if len(out) > MAX_OUTPUT_CHARS else "")
        + "```"
        for sid, desc, out in step_log
    )

    step_count = len(step_log)
    step_ids = " → ".join(sid for sid, _, _ in step_log)

    return (
        f"# Debug Report — ORIA Bloc 10 (Mode Orchestré)\n\n"
        f"**Task:** {task}\n"
        f"**Plan source:** `{plan_source}`\n"
        f"**Steps executed:** {step_count} ({step_ids})\n\n"
        f"## Execution Steps\n\n"
        f"{steps_md}\n\n"
        f"## Synthesis\n\n"
        f"{synthesis}\n\n"
        f"---\n"
        f"*Generated by debug-orchestrator v1.0.0 (ORIA bloc 10 debug agent)*\n"
    )


# ─────────────────────────────────────────────────────────────────────────────
# Module-level agent instance (required by Apollia AIP duck-typing)
# ─────────────────────────────────────────────────────────────────────────────

agent = DebugOrchestrator()

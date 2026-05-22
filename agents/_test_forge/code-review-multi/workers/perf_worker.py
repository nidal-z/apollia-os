"""Perf Worker — audit performance selon APOLLIA.md `Code Review — Performance Budget`."""

from __future__ import annotations

from typing import Any

from apollia import DomainError, agent, react, skill
from apollia.types import Ctx
from apollia.utils.parsing import extract_json


FALLBACK_PERF_BUDGET = """
Budget perf minimal (fallback) :
- Pas de N+1 query évident.
- Pas d'allocation dans les hot loops.
- IO : préférer batch/streaming aux opérations unitaires en boucle.
- Cache : signaler les calculs coûteux répétables non-mémoïsés.
- Algorithmie : signaler les O(n²) sur des collections potentiellement grandes.
"""


SYSTEM_PROMPT = """Tu es un reviewer performance senior.

Tu reçois un chemin de fichier et un budget perf (APOLLIA.md ou fallback).

Étapes :
1. Lis le fichier.
2. Repère hot paths, boucles, IO, allocations.
3. Évalue contre le budget.

Format JSON :
{
  "findings": [
    {"severity": "low|medium|high", "line": 42, "rule": "...", "description": "...", "recommendation": "..."}
  ],
  "summary": "N findings"
}
"""


@agent(
    name="perf-worker",
    version="0.1.0",
    description="Audit performance selon budget workspace",
    tags=("code-review", "performance", "worker"),
    agent_type="worker",
    memory_namespace="code-review-multi",
    tools_required=("file_read", "file_grep"),
    step_budget={"max_steps": 6, "max_tool_calls": 8, "wall_clock_secs": 180},
)
class PerfWorker:
    """Audit performance."""

    @skill("review.performance", description="Audit performance")
    async def review_performance(self, target_path: str, ctx: Ctx = None) -> dict[str, Any]:
        if ctx.llm is None:
            raise DomainError("NO_LLM", "Backend LLM requis")
        if not target_path.strip():
            raise DomainError("MISSING_TARGET", "target_path requis")

        budget = FALLBACK_PERF_BUDGET
        rules_source = "fallback"
        if ctx.workspace is not None:
            try:
                custom = ctx.workspace.get("Code Review — Performance Budget")
                if custom:
                    budget = custom
                    rules_source = "APOLLIA.md"
            except Exception as exc:
                ctx.logger.debug("workspace.get failed", error=str(exc))

        ctx.logger.info("perf review", target=target_path, source=rules_source)
        try:
            result = await react(
                ctx,
                system=SYSTEM_PROMPT
                + f"\n\n<perf_budget source='{rules_source}'>\n{budget}\n</perf_budget>",
                user=f"target_path: {target_path}",
                max_steps=6,
                temperature=0.2,
            )
        except DomainError:
            raise
        except Exception as exc:
            raise DomainError("EXECUTION_FAILED", str(exc)) from exc

        parsed = extract_json(result)
        if parsed is None:
            raise DomainError("PARSE_ERROR", "Output non parseable")
        return parsed

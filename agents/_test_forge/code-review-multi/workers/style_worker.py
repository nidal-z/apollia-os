"""Style Worker — audit style selon APOLLIA.md `Code Review — Style Guide`."""

from __future__ import annotations

import json
from typing import Any

from apollia.agents.react import AIPResult
from apollia.agents.worker import WorkerAgent
from apollia.utils.parsing import extract_json


FALLBACK_STYLE_GUIDE = """
Style guide minimal (fallback) :
- Naming : noms explicites, pas d'abréviations cryptiques.
- Fonctions courtes (<50 lignes), une responsabilité.
- Pas de duplication évidente.
- Commentaires uniquement quand le WHY est non-obvieux.
- Type hints (Python) ou types explicites (TS).
"""


class StyleWorker(WorkerAgent):
    SYSTEM_PROMPT = """Tu es un reviewer style/lisibilité senior.

Tu reçois un chemin de fichier et des règles de style depuis APOLLIA.md (ou fallback).

Étapes :
1. Lis le fichier (`file_read`).
2. Compare contre les règles.
3. Liste les écarts.

Format de sortie JSON :
{
  "findings": [
    {"severity": "low|medium|high", "line": 42, "rule": "...", "description": "...", "recommendation": "..."}
  ],
  "summary": "N findings"
}
"""
    MAX_STEPS = 6
    TEMPERATURE = 0.2

    def manifest(self) -> dict[str, Any]:
        return {
            "name": "style-worker",
            "version": "1.0.0",
            "description": "Audit style/lisibilité selon style guide workspace",
            "execution_mode": "direct",
            "agent_type": "user",
            "tools_required": ["file_read"],
            "memory_namespace": "code-review-multi",
            "supports_a2a": True,
            "max_concurrent_tasks": 2,
            "step_budget": {"max_steps": 6, "max_tool_calls": 8, "wall_clock_secs": 180},
            "skills": [
                {
                    "id": "review-style",
                    "description": "Audit style",
                    "input_modes": ["text"],
                    "output_modes": ["text"],
                    "input_schema": {"target_path": "str"},
                }
            ],
        }

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        if ctx.llm is None:
            return AIPResult.failed("NO_LLM", "Backend LLM requis")

        parts = task.get("input", {}).get("parts", [])
        raw = next((p.get("text", "") for p in parts if p.get("type") == "text"), "{}")
        try:
            payload = json.loads(raw) if raw.strip().startswith("{") else {"target_path": raw.strip()}
        except json.JSONDecodeError:
            payload = {"target_path": raw.strip()}

        target_path = payload.get("target_path", "").strip()
        if not target_path:
            return self.domain_error("missing_target", "target_path requis")

        rules = FALLBACK_STYLE_GUIDE
        rules_source = "fallback"
        if ctx.workspace is not None:
            try:
                custom = ctx.workspace.get("Code Review — Style Guide")
                if custom:
                    rules = custom
                    rules_source = "APOLLIA.md"
            except Exception as e:
                ctx.log("debug", f"workspace.get failed: {e}")

        ctx.log("info", f"style review {target_path} (rules: {rules_source})")

        user_message = json.dumps({"target_path": target_path}, ensure_ascii=False)
        extra_context = f"<style_guide source='{rules_source}'>\n{rules}\n</style_guide>"

        try:
            result = await self.react(task, ctx, user_message, extra_context=extra_context)
        except Exception as e:
            return self.domain_error("execution_failed", str(e))

        if isinstance(result, dict):
            return result

        parsed = extract_json(result)
        if parsed is None:
            return self.domain_error("parse_error", "Output non parseable")

        return AIPResult.completed(json.dumps(parsed, ensure_ascii=False))


agent = StyleWorker()


def manifest() -> dict[str, Any]:
    return agent.manifest()


async def run(task: dict[str, Any], ctx: Any) -> dict[str, Any]:
    return await agent.run(task, ctx)

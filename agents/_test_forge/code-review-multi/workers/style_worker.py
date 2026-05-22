"""Style Worker — audit style selon APOLLIA.md `Code Review — Style Guide`."""

from __future__ import annotations

from typing import Any

from apollia import DomainError, agent, react, skill
from apollia.types import Ctx
from apollia.utils.parsing import extract_json


FALLBACK_STYLE_GUIDE = """
Style guide minimal (fallback) :
- Naming : noms explicites, pas d'abréviations cryptiques.
- Fonctions courtes (<50 lignes), une responsabilité.
- Pas de duplication évidente.
- Commentaires uniquement quand le WHY est non-obvieux.
- Type hints (Python) ou types explicites (TS).
"""


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


@agent(
    name="style-worker",
    version="0.1.0",
    description="Audit style/lisibilité selon style guide workspace",
    tags=("code-review", "style", "worker"),
    agent_type="worker",
    memory_namespace="code-review-multi",
    tools_required=("file_read",),
    step_budget={"max_steps": 6, "max_tool_calls": 8, "wall_clock_secs": 180},
)
class StyleWorker:
    """Audit style/lisibilité."""

    @skill("review.style", description="Audit style")
    async def review_style(self, target_path: str, ctx: Ctx = None) -> dict[str, Any]:
        if ctx.llm is None:
            raise DomainError("NO_LLM", "Backend LLM requis")
        if not target_path.strip():
            raise DomainError("MISSING_TARGET", "target_path requis")

        rules = FALLBACK_STYLE_GUIDE
        rules_source = "fallback"
        if ctx.workspace is not None:
            try:
                custom = ctx.workspace.get("Code Review — Style Guide")
                if custom:
                    rules = custom
                    rules_source = "APOLLIA.md"
            except Exception as exc:
                ctx.logger.debug("workspace.get failed", error=str(exc))

        ctx.logger.info("style review", target=target_path, source=rules_source)
        try:
            result = await react(
                ctx,
                system=SYSTEM_PROMPT
                + f"\n\n<style_guide source='{rules_source}'>\n{rules}\n</style_guide>",
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

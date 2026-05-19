"""synthesis-worker — Synthèse + scoring + production VeilleReport JSON.

Worker A2A. Reçoit articles bruts + scoring + user_context, applique le
scoring (additif pondéré), produit un VeilleReport JSON conforme au
schema Pydantic (le director rend ensuite via Jinja2).

Skill A2A : ``research.synthesize_report``.
"""

from __future__ import annotations

import json
import re
from typing import Any

from apollia import DomainError, agent, react, skill
from apollia.types import Ctx
from apollia.utils.parsing import extract_json


SYSTEM_PROMPT = """Tu es synthesis-worker, expert en synthèse de veille technologique et concurrentielle.

<role>
À partir d'articles bruts, tu produis un rapport structuré JSON conforme au schema VeilleReport.
- Tu NE produis PAS de Markdown (le director rend via Jinja2).
- Tu retournes un JSON pur, validé Pydantic côté director.
- Tu appliques le scoring fourni (additif pondéré : pour chaque critère matché, ajouter weight).
- Tu écris l'executive_summary en 1-2 lignes maximum, en français, adapté au profil utilisateur si fourni.
</role>

<scoring_rules>
- score final ∈ [1, 5] : tu pondères selon les critères matchés dans `scoring.criteria`.
- Si le total dépasse 5, plafonne à 5. Si rien ne match, score=1.
- score_stars = représentation ★ : 5→"★★★★★", 4→"★★★★☆", 3→"★★★☆☆", 2→"★★☆☆☆", 1→"★☆☆☆☆"
- is_critical = true si score >= scoring.critical_threshold OR keyword critique présent.
- matched_criteria = liste des id de critères qui ont matché.
</scoring_rules>

<output_format>
JSON strict conforme au schema VeilleReport :
{
  "date": "YYYY-MM-DD",
  "executive_summary": "1-2 lignes",
  "articles_tech": [Article, ...],
  "articles_competitive": [Article, ...],
  "critical_findings": [Article, ...],
  "new_entities": [],
  "metrics": {}
}

Article = {
  "title": str (1-300 chars),
  "url": str,
  "source": str (hostname),
  "excerpt": str (max 2000 chars),
  "score": int (1-5),
  "score_stars": str (5 chars étoiles),
  "axis": "tech" | "competitive",
  "entities": [str ids "entity:type:id"],
  "impact_apollia": str (1-2 phrases sur l'impact pour Apollia OS),
  "impact_for_user": str (vide ou 1 phrase si user_context permet personnalisation),
  "is_critical": bool,
  "matched_criteria": [str ids critères matchés]
}
</output_format>

<rules>
1. Retourne UNIQUEMENT du JSON valide, pas de Markdown, pas de texte avant/après.
2. Filtrer les articles avec score < scoring.include_threshold (défaut 2).
3. Limiter chaque axe à max 6 articles (les top par score).
4. Si user_context.user_role est "CTO" ou similaire, executive_summary plus tech ; si "founder", plus business.
5. Conserver la langue des excerpts (FR ou EN selon source).
</rules>
"""


@agent(
    name="synthesis-worker",
    version="3.0.0",
    description="Scoring + synthèse VeilleReport JSON conforme schema Pydantic.",
    tags=("worker", "synthesis", "scoring"),
    agent_type="worker",
    memory_namespace="veille-ia",
    packages=("pydantic",),
    step_budget={"max_steps": 8, "max_tool_calls": 0, "wall_clock_secs": 300},
)
class SynthesisWorker:
    """Worker A2A — scoring + synthèse VeilleReport JSON."""

    @skill(
        "research.synthesize_report",
        description=(
            "Score les articles et produit un VeilleReport JSON validable Pydantic."
        ),
    )
    async def synthesize_report(
        self,
        articles_tech: list[dict[str, Any]],
        articles_competitive: list[dict[str, Any]],
        scoring: dict[str, Any],
        user_context: dict[str, Any] | None = None,
        article_to_entities: dict[str, Any] | None = None,
        date: str = "",
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Synthèse VeilleReport JSON."""
        if ctx.llm is None:
            raise DomainError("NO_LLM", "Backend LLM requis")

        if not articles_tech and not articles_competitive:
            return {
                "date": date,
                "executive_summary": "Aucun article significatif aujourd'hui.",
                "articles_tech": [],
                "articles_competitive": [],
                "critical_findings": [],
                "new_entities": [],
                "metrics": {},
            }

        user_message = json.dumps(
            {
                "date": date,
                "articles_tech": articles_tech,
                "articles_competitive": articles_competitive,
                "scoring": scoring,
                "user_context": user_context or {},
                "article_to_entities": article_to_entities or {},
            },
            ensure_ascii=False,
        )

        try:
            result = await react(
                ctx,
                system=SYSTEM_PROMPT,
                user=user_message,
                max_steps=8,
                temperature=0.2,
            )
        except DomainError:
            raise
        except Exception as exc:
            raise DomainError("EXECUTION_FAILED", str(exc)) from exc

        cleaned = re.sub(
            r"^```(?:json)?\s*|\s*```$", "", result.strip(), flags=re.MULTILINE
        ).strip()
        parsed = extract_json(cleaned) if not cleaned.startswith("{") else None
        if parsed is None:
            try:
                parsed = json.loads(cleaned)
            except json.JSONDecodeError as exc:
                raise DomainError(
                    "PARSE_ERROR", f"Output non parseable JSON: {exc}"
                ) from exc

        return parsed

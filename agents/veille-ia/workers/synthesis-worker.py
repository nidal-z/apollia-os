"""synthesis-worker v3.0.0 — Synthèse + scoring + production VeilleReport JSON.

Refonte v3 :
- Reçoit articles bruts + scoring.yaml + user_context + article_to_entities depuis le director.
- Applique le scoring (additif pondéré sur critères matchés).
- LLM produit un VeilleReport JSON conforme schemas.py (Pydantic).
- Retourne le JSON validé (le director rend via Jinja2, ce worker NE FAIT PAS de Markdown).

Skill A2A : `synthesize-report`.
"""

from __future__ import annotations

import json
import re
from typing import Any

from apollia.agents.react import AIPResult
from apollia.agents.worker import WorkerAgent
from apollia.utils.parsing import extract_json


class SynthesisWorker(WorkerAgent):
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
  "critical_findings": [Article, ...],   // articles avec is_critical=true
  "new_entities": [],                      // remplis par le director, laisser []
  "metrics": {}                            // remplis par le director, laisser {}
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
    MAX_STEPS = 8
    TEMPERATURE = 0.2

    def manifest(self) -> dict[str, Any]:
        return {
            "name": "synthesis-worker",
            "version": "3.0.0",
            "description": "Scoring + synthèse VeilleReport JSON conforme schema Pydantic.",
            "execution_mode": "direct",
            "agent_type": "user",
            "tools_required": [],
            "memory_namespace": "veille-ia",
            "shared_memory_namespaces": [],
            "supports_a2a": True,
            "max_concurrent_tasks": 2,
            "step_budget": {"max_steps": 8, "max_tool_calls": 0, "wall_clock_secs": 300},
            "skills": [
                {
                    "id": "synthesize-report",
                    "name": "Synthétiser le rapport de veille",
                    "description": "Score les articles et produit un VeilleReport JSON validable Pydantic.",
                    "input_modes": ["text"],
                    "output_modes": ["text"],
                    "input_schema": {
                        "articles_tech": "array",
                        "articles_competitive": "array",
                        "scoring": "object",
                        "user_context": "object",
                        "article_to_entities": "object",
                        "date": "string",
                    },
                }
            ],
            "packages": ["pydantic"],
            "tags": ["worker", "synthesis", "scoring"],
        }

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        if ctx.llm is None:
            return AIPResult.failed("NO_LLM", "Backend LLM requis")

        parts = task.get("input", {}).get("parts", [])
        raw = next((p["text"] for p in parts if p.get("type") == "text"), "{}")
        try:
            payload = json.loads(raw) if raw.strip().startswith("{") else {"text": raw}
        except json.JSONDecodeError:
            return self.domain_error("invalid_payload", "Payload non parseable en JSON")

        articles_tech = payload.get("articles_tech", [])
        articles_competitive = payload.get("articles_competitive", [])
        scoring = payload.get("scoring", {})
        user_context = payload.get("user_context", {})
        article_to_entities = payload.get("article_to_entities", {})
        date = payload.get("date", "")

        if not articles_tech and not articles_competitive:
            empty_report = {
                "date": date,
                "executive_summary": "Aucun article significatif aujourd'hui.",
                "articles_tech": [],
                "articles_competitive": [],
                "critical_findings": [],
                "new_entities": [],
                "metrics": {},
            }
            return AIPResult.completed(json.dumps(empty_report, ensure_ascii=False))

        # Construire le user_message structuré
        user_message = json.dumps(
            {
                "date": date,
                "articles_tech": articles_tech,
                "articles_competitive": articles_competitive,
                "scoring": scoring,
                "user_context": user_context,
                "article_to_entities": article_to_entities,
            },
            ensure_ascii=False,
        )

        try:
            result = await self.react(task, ctx, user_message)
        except Exception as e:
            return self.domain_error("execution_failed", str(e))

        if isinstance(result, dict):
            return result

        # Parse JSON output structuré
        # Strip code fences si LLM en a ajouté
        cleaned = re.sub(r"^```(?:json)?\s*|\s*```$", "", result.strip(), flags=re.MULTILINE).strip()
        parsed = extract_json(cleaned) if not cleaned.startswith("{") else None
        if parsed is None:
            try:
                parsed = json.loads(cleaned)
            except json.JSONDecodeError as e:
                return self.domain_error("parse_error", f"Output non parseable JSON: {e}")

        return AIPResult.completed(json.dumps(parsed, ensure_ascii=False))


# Variable module-level requise par le runtime Apollia (loader.rs:113-115).
agent = SynthesisWorker()

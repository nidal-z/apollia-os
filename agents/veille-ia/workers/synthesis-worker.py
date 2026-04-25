"""synthesis-worker — Worker Agent for watch report synthesis.

Specialised agent that analyses a list of articles and produces a structured
Markdown watch report. Exposes the ``synthesize-report`` A2A skill.

No external tools needed — works entirely via LLM calls.

Input payload (via A2A skill "synthesize-report"):
  articles       list[dict]  — Articles to analyse (title, url, excerpt, axis, source)
  date           str         — ISO date string (e.g. "2026-04-24")
  axes           dict        — Axis definitions {"tech": "...", "competitive": "..."}

Output:
  report_markdown  str         — Full Markdown report
  summary          str         — 1-2 line executive summary
  top_items        list[dict]  — Top 3 items with title, url, score, axis
  article_count    int         — Number of articles analysed
"""

from __future__ import annotations

import json
from typing import Any

from apollia.agents import AIPResult, WorkerAgent

SYSTEM_PROMPT: str = """\
Tu es synthesis-worker, un agent spécialisé dans l'analyse et la synthèse de \
veille technologique et concurrentielle pour Apollia OS.

## RÔLE

Tu reçois une liste d'articles bruts et tu dois produire un rapport de veille \
structuré, clair, et actionnable pour les équipes d'Apollia OS.

## STRUCTURE DU RAPPORT

Le rapport doit suivre ce format Markdown exact :

```
# Veille IA/LLM — {date_formatée}

> {résumé_exécutif_1_2_lignes}

---

## Veille Technologique

{articles_tech_triés_par_pertinence}

---

## Veille Concurrentielle

{articles_concurrentiels_triés_par_pertinence}

---

## Résumé

| Métrique | Valeur |
|---|---|
| Articles analysés | N |
| Nouveaux articles | M |
| Score moyen | X.X/5 |

*Rapport généré par veille-ia-agent · Apollia OS*
```

## FORMAT D'UN ARTICLE DANS LE RAPPORT

Pour chaque article, utilise ce format :

### {titre} ★★★★☆ (score/5)
> {excerpt_résumé_1_2_phrases}
**Source :** {domaine} · **[Lire l'article →]({url})**

Pour les articles de score 4-5 : ajouter une ligne "**Pourquoi c'est important :**"
avec 1-2 phrases d'analyse pour les équipes Apollia.

## SCORING DE PERTINENCE (1-5 étoiles)

- 5 ★ : Impact direct sur Apollia (nouveau concurrent majeur, breakthrough technique,
        funding concurrent, changement de standard MCP/A2A)
- 4 ★ : Fort intérêt pour la roadmap ou la compétition
- 3 ★ : Pertinent mais pas urgent
- 2 ★ : Information de fond
- 1 ★ : Peu pertinent, inclure si peu d'articles disponibles

**Ne jamais inclure les articles de score 1 si d'autres sont disponibles.**

## RÈGLES DE RÉDACTION

1. Trier les articles par score décroissant dans chaque axe.
2. Le résumé exécutif doit mentionner les 2-3 éléments les plus importants
   de la session.
3. Écrire en français pour les analyses. Les titres d'articles peuvent rester
   en anglais.
4. Ne jamais inventer d'informations — se baser uniquement sur les excerpts fournis.
5. Si un axe n'a aucun article : écrire "Aucun article notable pour cette période."

## FORMAT DE RÉPONSE

Après avoir analysé les articles, fournis un JSON :
{
  "report_markdown": "...",
  "summary": "...",
  "top_items": [
    {"title": "...", "url": "...", "score": 5, "axis": "tech"},
    ...
  ],
  "article_count": N
}
"""


def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for synthesis-worker."""
    return {
        "name": "synthesis-worker",
        "version": "1.0.0",
        "description": (
            "Worker spécialisé dans la synthèse de rapports de veille IA/LLM. "
            "Analyse une liste d'articles bruts, les score par pertinence, et "
            "produit un rapport Markdown structuré avec résumé exécutif. "
            "Fonctionne uniquement via LLM — aucun outil externe requis."
        ),
        "execution_mode": "direct",
        "agent_type": "worker",
        "tools_required": [],
        "tools_optional": [],
        "tools_requiring_approval": [],
        "packages": [],
        "supports_a2a": True,
        "step_budget": {
            "max_steps": 10,
            "max_tool_calls": 5,
            "wall_clock_secs": 180,
        },
        "skills": [
            {
                "id": "synthesize-report",
                "name": "Synthétiser un rapport de veille",
                "description": (
                    "Analyse une liste d'articles (title, url, excerpt, axis, source), "
                    "les score par pertinence pour Apollia OS, et produit un rapport "
                    "Markdown structuré avec résumé exécutif et top items."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "articles": {
                        "type": "array",
                        "description": "Articles à analyser",
                        "required": True,
                    },
                    "date": {
                        "type": "string",
                        "description": "Date du rapport (ISO format)",
                        "required": True,
                    },
                    "axes": {
                        "type": "object",
                        "description": "Définitions des axes de veille",
                        "required": False,
                    },
                },
            }
        ],
        "tags": ["synthesis", "report", "watch", "analysis", "worker"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
    }


class SynthesisWorker(WorkerAgent):
    """Worker Agent for watch report synthesis.

    Receives raw articles from web-search-worker, scores them by relevance
    to Apollia OS (1-5 stars), and produces a structured Markdown report.
    Works entirely via LLM — no external tool calls needed.
    """

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 8
    TEMPERATURE = 0.3

    def manifest(self) -> dict[str, Any]:
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Synthesize a watch report from the provided articles."""
        payload = (
            task.get("input", {})
            if isinstance(task.get("input"), dict)
            else {}
        )

        articles: list[dict] = payload.get("articles", [])
        date: str = payload.get("date", "")
        axes: dict = payload.get("axes", {
            "tech": "Actualités techniques IA/LLM",
            "competitive": "Actualités concurrentielles",
        })

        if not articles:
            return AIPResult.completed(json.dumps({
                "report_markdown": (
                    f"# Veille IA/LLM — {date}\n\n"
                    "> Aucun article trouvé pour cette session.\n"
                ),
                "summary": "Aucun article trouvé pour cette session.",
                "top_items": [],
                "article_count": 0,
            }, ensure_ascii=False))

        tech_articles = [a for a in articles if a.get("axis") == "tech"]
        competitive_articles = [a for a in articles if a.get("axis") == "competitive"]

        user_message = (
            f"Génère le rapport de veille du {date}.\n\n"
            f"**Axes de veille :**\n"
            + "\n".join(f"- {k}: {v}" for k, v in axes.items())
            + f"\n\n**Articles tech ({len(tech_articles)}) :**\n"
            + json.dumps(tech_articles, ensure_ascii=False, indent=2)
            + f"\n\n**Articles concurrentiels ({len(competitive_articles)}) :**\n"
            + json.dumps(competitive_articles, ensure_ascii=False, indent=2)
            + "\n\nAnalyse et score chaque article. Génère le rapport Markdown complet "
            "puis retourne le JSON avec report_markdown, summary, top_items et article_count."
        )

        result = await self.react(task, ctx, user_message)
        if isinstance(result, dict):
            return result

        from apollia.utils.parsing import extract_json

        parsed = extract_json(result)
        if parsed and "report_markdown" in parsed:
            return AIPResult.completed(json.dumps(parsed, ensure_ascii=False))

        # If the LLM returned raw Markdown directly instead of JSON
        return AIPResult.completed(json.dumps({
            "report_markdown": result,
            "summary": result.split("\n")[0].lstrip("#> "),
            "top_items": [],
            "article_count": len(articles),
        }, ensure_ascii=False))


agent = SynthesisWorker()

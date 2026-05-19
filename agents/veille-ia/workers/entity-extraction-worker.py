"""entity-extraction-worker — Extraction d'entités depuis articles.

Worker A2A : reçoit articles + known_entities, retourne entités structurées
(companies, products, events, topics) + mapping article URL → entity IDs.

Skill A2A : ``research.extract_entities``.
"""

from __future__ import annotations

import json
import re
from typing import Any

from apollia import DomainError, agent, react, skill
from apollia.types import Ctx
from apollia.utils.parsing import extract_json


SYSTEM_PROMPT = """Tu es entity-extraction-worker, expert en extraction d'entités à partir d'articles de veille IA.

<role>
À partir d'une liste d'articles, tu identifies les entités présentes (companies, products, events, topics) et les retournes structurées.
</role>

<entity_types>
- company : organisations IA (Anthropic, Mistral, Dust, Lindy, n8n, OpenAI, etc.)
- product : produits/frameworks identifiés (Claude Code, GPT-5, Gemini Enterprise, MCP, A2A protocol, etc.)
- event : événements ponctuels datés (levée de fonds, acquisition, release, conférence)
- topic : sujet récurrent (agentic memory, context engineering, EU AI Act, etc.)
</entity_types>

<output_format>
JSON :
{
  "entities": [
    {
      "id": "kebab-case-unique",
      "type": "company" | "product" | "event" | "topic",
      "name": "Nom officiel",
      "category": "direct" | "indirect" | "neutral",
      "threat_level": "low" | "medium" | "high",
      "summary": "1-2 phrases synthèse de ce qui a été dit dans les articles",
      "score": int 1-5 (importance dans le run),
      "source_url": "URL de la source principale"
    }
  ],
  "article_to_entities": {
    "https://article-url-1": ["entity:company:n8n", "entity:product:mcp"],
    "https://article-url-2": [...]
  }
}
</output_format>

<rules>
1. Ne PAS inventer d'entités absentes des articles.
2. Réutiliser les IDs de `known_entities` si l'entité est déjà connue.
3. Limiter à 15 entités max par run (les plus saillantes).
4. id = kebab-case ASCII (ex: "anthropic", "claude-code", "series-b-2026-04-anthropic").
5. category/threat_level uniquement pour type=company. Sinon : category="neutral", threat_level="low".
6. Réponds UNIQUEMENT avec le JSON, pas de Markdown ni de texte avant/après.
</rules>
"""


@agent(
    name="entity-extraction-worker",
    version="1.0.0",
    description="Extraction d'entités structurées depuis articles de veille.",
    tags=("worker", "entity-extraction", "ner"),
    agent_type="worker",
    memory_namespace="veille-ia",
    step_budget={"max_steps": 6, "max_tool_calls": 0, "wall_clock_secs": 180},
)
class EntityExtractionWorker:
    """Worker A2A — extraction d'entités depuis articles."""

    @skill(
        "research.extract_entities",
        description=(
            "Extrait les entités (companies, products, events, topics) "
            "depuis articles."
        ),
    )
    async def extract_entities(
        self,
        articles: list[dict[str, Any]],
        known_entities: list[dict[str, Any]] | None = None,
        today: str = "",
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Extrait entités structurées + mapping article→entities."""
        if ctx.llm is None:
            raise DomainError("NO_LLM", "Backend LLM requis")

        if not articles:
            return {"entities": [], "article_to_entities": {}}

        user_message = json.dumps(
            {
                "today": today,
                "articles": articles,
                "known_entities": known_entities or [],
            },
            ensure_ascii=False,
        )

        try:
            result = await react(
                ctx,
                system=SYSTEM_PROMPT,
                user=user_message,
                max_steps=6,
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

        parsed.setdefault("entities", [])
        parsed.setdefault("article_to_entities", {})
        return parsed

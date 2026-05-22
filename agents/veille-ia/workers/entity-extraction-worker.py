"""entity-extraction-worker — Extraction d'entités depuis articles.

Worker A2A : reçoit articles + known_entities, retourne entités structurées
(companies, products, events, topics) + mapping article URL → entity IDs.

Skill A2A : ``research.extract_entities``.
"""

# ⚠️ Pas de ``from __future__ import annotations`` : les signatures
# référencent des TypedDicts définis dans ``worker_schemas`` et le SDK
# Apollia introspecte ``TypedDict.__required_keys__`` qui se casse sous
# PEP 563 (toutes les clés deviendraient requises). cf. ADR-099.

import json
import re
import sys
from pathlib import Path
from typing import Annotated, Any

from apollia import DomainError, agent, react, skill
from apollia.types import Ctx
from apollia.utils.parsing import extract_json

# Le worker vit dans agents/veille-ia/workers/ ; ``worker_schemas`` est un
# cran au-dessus (agents/veille-ia/). On insère le dossier parent dans
# sys.path AVANT l'import pour que le runtime PyO3 (qui charge via
# ``PyModule::from_code``) résolve l'import absolu.
_AGENT_DIR = Path(__file__).resolve().parent.parent
if str(_AGENT_DIR) not in sys.path:
    sys.path.insert(0, str(_AGENT_DIR))

# Force purge de cache module si un autre agent a déjà importé ses propres
# ``worker_schemas`` (le bridge invalide ``apollia.*`` à chaque load mais
# pas les modules d'agent — cf. veille-ia-agent.py).
if "worker_schemas" in sys.modules:
    del sys.modules["worker_schemas"]

from worker_schemas import (  # noqa: E402
    Article,
    ArticleEntityMap,
    Entity,
)


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
    version="0.1.0",
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
        examples=[
            {
                "today": "2026-05-20",
                "articles": [
                    {
                        "title": "Anthropic launches Claude Code 2.0",
                        "url": "https://example.com/anthropic-claude-code-2",
                        "source": "example.com",
                        "excerpt": "Anthropic announced today...",
                        "axis": "tech",
                    },
                ],
                "known_entities": [
                    {"id": "anthropic", "type": "company", "name": "Anthropic"},
                ],
            },
        ],
    )
    async def extract_entities(
        self,
        articles: Annotated[
            list[Article],
            "Articles de veille (sortie de research.search_and_extract). "
            "Chaque article a au minimum {title, url, source, excerpt, axis}.",
        ],
        known_entities: Annotated[
            list[Entity] | None,
            "Entités déjà connues en mémoire (à réutiliser plutôt que recréer). "
            "Format : [{id, type, name, ...}].",
        ] = None,
        today: Annotated[
            str,
            "Date ISO (YYYY-MM-DD) du run en cours — utilisée pour dater les signaux.",
        ] = "",
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Extrait entités structurées + mapping article→entities.

        Retourne un dict ``{entities: [...], article_to_entities: {url: [entity_ids]}}``
        conforme à ``worker_schemas.ExtractEntitiesOutput``.
        """
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
        # Mapping article → entities : ``ArticleEntityMap`` est un alias
        # de ``dict[str, list[str]]`` (clés URL dynamiques, pas de TypedDict).
        _: ArticleEntityMap = parsed["article_to_entities"]
        return parsed

"""entity-extraction-worker v1.0.0 — Extraction d'entités depuis articles.

NOUVEAU worker introduit en v3.0.0 (M5a release).

Reçoit une liste d'articles + liste des known_entities (clés en mémoire), et retourne :
- Une liste d'entités structurées (companies, products, events, topics).
- Un mapping article URL → entity IDs.

Le director persiste ensuite ces entités via `entity:{type}:{id}` avec timeline.

Skill A2A : `extract-entities`.
"""

from __future__ import annotations

import json
import re
from typing import Any

from apollia.agents.react import AIPResult
from apollia.agents.worker import WorkerAgent
from apollia.utils.a2a import extract_a2a_payload
from apollia.utils.parsing import extract_json


class EntityExtractionWorker(WorkerAgent):
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
      "category": "direct" | "indirect" | "neutral",  // pour companies seulement, sinon "neutral"
      "threat_level": "low" | "medium" | "high",       // pour companies seulement, sinon "low"
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
2. Réutiliser les IDs de `known_entities` si l'entité est déjà connue (ne pas créer un doublon avec un id légèrement différent).
3. Limiter à 15 entités max par run (les plus saillantes).
4. id = kebab-case ASCII (ex: "anthropic", "claude-code", "series-b-2026-04-anthropic").
5. category/threat_level uniquement pour type=company. Sinon : category="neutral", threat_level="low".
6. Réponds UNIQUEMENT avec le JSON, pas de Markdown ni de texte avant/après.
</rules>
"""
    MAX_STEPS = 6
    TEMPERATURE = 0.2

    def manifest(self) -> dict[str, Any]:
        return {
            "name": "entity-extraction-worker",
            "version": "1.0.0",
            "description": "Extraction d'entités structurées depuis articles de veille.",
            "execution_mode": "direct",
            "agent_type": "user",
            "tools_required": [],
            "memory_namespace": "veille-ia",
            "shared_memory_namespaces": [],
            "supports_a2a": True,
            "max_concurrent_tasks": 2,
            "step_budget": {"max_steps": 6, "max_tool_calls": 0, "wall_clock_secs": 180},
            "skills": [
                {
                    "id": "extract-entities",
                    "name": "Extraire les entités des articles",
                    "description": "Extrait les entités (companies, products, events, topics) depuis articles.",
                    "input_modes": ["text"],
                    "output_modes": ["text"],
                    "input_schema": {
                        "articles": "array",
                        "known_entities": "array",
                        "today": "string",
                    },
                }
            ],
            "packages": [],
            "tags": ["worker", "entity-extraction", "ner"],
        }

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        if ctx.llm is None:
            return AIPResult.failed("NO_LLM", "Backend LLM requis")

        payload = extract_a2a_payload(task)
        if not payload:
            return self.domain_error("invalid_payload", "Payload A2A vide ou non parseable")

        articles = payload.get("articles", [])
        known_entities = payload.get("known_entities", [])
        today = payload.get("today", "")

        if not articles:
            empty_result = {"entities": [], "article_to_entities": {}}
            return AIPResult.completed(json.dumps(empty_result, ensure_ascii=False))

        user_message = json.dumps(
            {
                "today": today,
                "articles": articles,
                "known_entities": known_entities,
            },
            ensure_ascii=False,
        )

        try:
            result = await self.react(task, ctx, user_message)
        except Exception as e:
            return self.domain_error("execution_failed", str(e))

        if isinstance(result, dict):
            return result

        cleaned = re.sub(r"^```(?:json)?\s*|\s*```$", "", result.strip(), flags=re.MULTILINE).strip()
        parsed = extract_json(cleaned) if not cleaned.startswith("{") else None
        if parsed is None:
            try:
                parsed = json.loads(cleaned)
            except json.JSONDecodeError as e:
                return self.domain_error("parse_error", f"Output non parseable JSON: {e}")

        # Sanity check : structure
        if "entities" not in parsed:
            parsed["entities"] = []
        if "article_to_entities" not in parsed:
            parsed["article_to_entities"] = {}

        return AIPResult.completed(json.dumps(parsed, ensure_ascii=False))


# Variable module-level requise par le runtime Apollia (loader.rs:113-115).
agent = EntityExtractionWorker()

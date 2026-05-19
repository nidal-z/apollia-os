"""web-search-worker — Worker Agent for web search and content extraction.

Specialised agent that performs web searches and extracts article content.
Exposes the ``search-and-extract`` A2A skill, used by director agents (e.g.
veille-ia-agent) to delegate research tasks.

Required tools (fail-fast at agent initialisation):
  web_search, web_read

Input payload (via A2A skill "search-and-extract"):
  queries        list[str]   — Search queries to run
  axis           str         — Watch axis label ("tech" or "competitive")
  seen_hashes    list[str]   — URL hashes already seen (for deduplication)
  max_articles   int         — Max articles to return (default 8)

Output:
  articles       list[dict]  — Extracted articles with title, url, excerpt, hash, source, axis
  total_found    int         — Total results found before dedup
  skipped_dupes  int         — Articles skipped because already seen
"""

from __future__ import annotations

import hashlib
import json
from typing import Any
from urllib.parse import urlparse

from apollia.agents import AIPResult, WorkerAgent
from apollia.utils.a2a import extract_a2a_payload

SYSTEM_PROMPT: str = """\
Tu es web-search-worker, un agent spécialisé dans la recherche web et l'extraction \
de contenu d'articles.

## RÔLE

Tu reçois une liste de requêtes de recherche et un axe de veille. Tu dois :
1. Exécuter chaque requête avec web_search
2. Filtrer les résultats déjà vus (selon seen_hashes)
3. Extraire le contenu des articles pertinents avec web_read
4. Retourner une liste structurée d'articles

## RÈGLES ABSOLUES

1. Toujours vérifier les seen_hashes AVANT d'appeler web_read — ne pas lire
   des pages inutilement.

2. Limiter les appels web_read aux articles les plus pertinents selon le titre
   et l'extrait. Si un titre est clairement hors-sujet, skip.

3. Pour chaque article lu : extraire titre, un excerpt de 200-300 mots maximum
   (premier paragraphe substantiel), et la source (domaine).

4. Si web_search retourne une erreur ou 0 résultats : continuer avec la query
   suivante, ne pas bloquer.

5. Si web_read échoue pour une URL : logger l'erreur dans la pensée et
   continuer, ne pas bloquer.

## FORMAT DE RÉPONSE FINAL

Quand tu as fini toutes les recherches, réponds avec un JSON contenant :
{
  "articles": [
    {
      "title": "...",
      "url": "https://...",
      "excerpt": "...",
      "source": "techcrunch.com",
      "hash": "abc123def456",
      "axis": "tech"
    }
  ],
  "total_found": 15,
  "skipped_dupes": 3
}

## LANGUE

Toujours répondre en français dans les pensées. Les excerpts sont dans la langue
de l'article original (anglais le plus souvent).
"""


def _url_hash(url: str) -> str:
    """Return a 12-char SHA-256 prefix of the URL (stable dedup key)."""
    return hashlib.sha256(url.encode()).hexdigest()[:12]


def _extract_domain(url: str) -> str:
    """Extract the hostname from a URL, stripping www. prefix."""
    try:
        host = urlparse(url).hostname or url
        return host.removeprefix("www.")
    except Exception:
        return url


def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for web-search-worker."""
    return {
        "name": "web-search-worker",
        "version": "1.0.0",
        "description": (
            "Worker spécialisé dans la recherche web et l'extraction de contenu. "
            "Effectue des recherches sur des requêtes données, déduplique les résultats "
            "par hash d'URL, et extrait le contenu des articles les plus pertinents. "
            "Utilisé comme sous-agent par les agents de veille."
        ),
        "execution_mode": "direct",
        "agent_type": "worker",
        "tools_required": ["web_search", "web_read"],
        "tools_optional": [],
        "tools_requiring_approval": [],
        "packages": [],
        "supports_a2a": True,
        "step_budget": {
            "max_steps": 30,
            "max_tool_calls": 60,
            "wall_clock_secs": 300,
        },
        "skills": [
            {
                "id": "search-and-extract",
                "name": "Rechercher et extraire",
                "description": (
                    "Effectue des recherches web sur une liste de requêtes, filtre les "
                    "articles déjà vus par hash d'URL, extrait le contenu des articles "
                    "pertinents. Retourne une liste structurée d'articles avec titre, "
                    "excerpt, URL, source et axe."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "queries": {
                        "type": "array",
                        "description": "Liste de requêtes de recherche à exécuter",
                        "required": True,
                    },
                    "axis": {
                        "type": "string",
                        "description": "Axe de veille : 'tech' ou 'competitive'",
                        "required": True,
                    },
                    "seen_hashes": {
                        "type": "array",
                        "description": "Hashes d'URLs déjà vus (pour la déduplication)",
                        "required": False,
                    },
                    "max_articles": {
                        "type": "integer",
                        "description": "Nombre maximum d'articles à retourner (défaut 8)",
                        "required": False,
                    },
                },
            }
        ],
        "tags": ["web", "search", "extraction", "research", "worker"],
        "max_concurrent_tasks": 2,
        "dangerous_tools_allowed": False,
    }


class WebSearchWorker(WorkerAgent):
    """Worker Agent for web search and article extraction.

    Operates the full search-deduplicate-extract pipeline:
    1. Run each query through web_search
    2. Skip URLs whose hash is in seen_hashes (cross-session dedup)
    3. Call web_read on new, relevant URLs to extract content
    4. Return structured article list

    The ReAct loop in BaseReActAgent handles the tool calling sequence.
    The SYSTEM_PROMPT encodes the rules to make this reliable on 7B+ models.
    """

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 30
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Execute the search-and-extract task."""
        payload = extract_a2a_payload(task)

        queries: list[str] = payload.get("queries", [])
        axis: str = payload.get("axis", "tech")
        seen_hashes: list[str] = payload.get("seen_hashes", [])
        max_articles: int = int(payload.get("max_articles", 8))

        if not queries:
            return AIPResult.failed(
                "NO_QUERIES",
                "web-search-worker: 'queries' est requis dans le payload",
            )

        seen_set = set(seen_hashes)

        # Build the task message for the ReAct loop
        user_message = (
            f"Exécute une veille web sur l'axe '{axis}'.\n\n"
            f"Requêtes à traiter ({len(queries)}):\n"
            + "\n".join(f"- {q}" for q in queries)
            + f"\n\nHashes déjà vus (à ignorer) : {json.dumps(list(seen_set))}\n"
            f"Maximum d'articles à retourner : {max_articles}\n\n"
            "Pour chaque URL nouvelle, calcule son hash (sha256 des 12 premiers "
            "caractères) et marque l'axe comme "
            + repr(axis)
            + ".\n"
            "Retourne le JSON final avec la liste des articles."
        )

        result = await self.react(task, ctx, user_message)
        if isinstance(result, dict):
            return result

        # Parse JSON from the final answer text
        from apollia.utils.parsing import extract_json

        parsed = extract_json(result)
        if parsed and "articles" in parsed:
            # Enrich articles with stable hashes if missing
            for article in parsed["articles"]:
                if not article.get("hash"):
                    article["hash"] = _url_hash(article.get("url", ""))
                if not article.get("source"):
                    article["source"] = _extract_domain(article.get("url", ""))
                article.setdefault("axis", axis)
            return AIPResult.completed(json.dumps(parsed, ensure_ascii=False))

        # Fallback: return raw text (shouldn't happen with a good model)
        return AIPResult.completed(json.dumps({
            "articles": [],
            "total_found": 0,
            "skipped_dupes": 0,
            "raw": result,
        }, ensure_ascii=False))


agent = WebSearchWorker()

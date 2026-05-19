"""web-search-worker — Worker A2A pour recherche web + extraction.

Worker spécialisé qui effectue des recherches web et extrait le contenu
des articles trouvés. Exposé via le skill A2A
``research.search_and_extract``, consommé par les directors (veille-ia,
…).

SDK : decorator-first Apollia AgentKit (ADR-098..ADR-112).
"""

from __future__ import annotations

import hashlib
import json
from typing import Any
from urllib.parse import urlparse

from apollia import DomainError, agent, react, skill
from apollia.types import Ctx
from apollia.utils.parsing import extract_json


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

1. Toujours vérifier les seen_hashes AVANT d'appeler web_read.
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
    return hashlib.sha256(url.encode()).hexdigest()[:12]


def _extract_domain(url: str) -> str:
    try:
        host = urlparse(url).hostname or url
        return host.removeprefix("www.")
    except Exception:
        return url


@agent(
    name="web-search-worker",
    version="1.0.0",
    description=(
        "Worker spécialisé dans la recherche web et l'extraction de contenu. "
        "Effectue des recherches sur des requêtes données, déduplique par hash "
        "d'URL, et extrait le contenu des articles pertinents."
    ),
    tags=("web", "search", "extraction", "research", "worker"),
    agent_type="worker",
    tools_required=("web_search", "web_read"),
    step_budget={"max_steps": 30, "max_tool_calls": 60, "wall_clock_secs": 300},
)
class WebSearchWorker:
    """Worker A2A pour la recherche web + extraction d'articles."""

    @skill(
        "research.search_and_extract",
        description=(
            "Effectue des recherches web, déduplique par hash d'URL, "
            "extrait le contenu des articles pertinents."
        ),
    )
    async def search_and_extract(
        self,
        queries: list[str],
        axis: str,
        seen_hashes: list[str] | None = None,
        max_articles: int = 8,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Recherche + extraction d'articles."""
        if ctx.llm is None:
            raise DomainError("NO_LLM", "Backend LLM requis")
        if not queries:
            raise DomainError("NO_QUERIES", "'queries' est requis dans le payload")

        seen_set = set(seen_hashes or [])
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

        tools: list[dict[str, Any]] = []
        if ctx.tools is not None:
            for tool_name in ("web_search", "web_read"):
                try:
                    tools.append(ctx.tools.describe(tool_name))
                except Exception as exc:
                    ctx.logger.warning(
                        "tool descriptor missing", tool=tool_name, error=str(exc)
                    )

        try:
            result = await react(
                ctx,
                system=SYSTEM_PROMPT,
                user=user_message,
                tools=tools,
                max_steps=30,
                temperature=0.1,
            )
        except DomainError:
            raise
        except Exception as exc:
            ctx.logger.error("react failed", error=str(exc))
            raise DomainError("REACT_FAILED", str(exc)) from exc

        parsed = extract_json(result)
        if parsed and "articles" in parsed:
            for article in parsed["articles"]:
                if not article.get("hash"):
                    article["hash"] = _url_hash(article.get("url", ""))
                if not article.get("source"):
                    article["source"] = _extract_domain(article.get("url", ""))
                article.setdefault("axis", axis)
            return parsed

        return {
            "articles": [],
            "total_found": 0,
            "skipped_dupes": 0,
            "raw": result,
        }

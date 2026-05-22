"""Markdown Summarizer — résume une URL en markdown adapté au profil utilisateur.

Pattern : L1 standalone (decorator-first SDK).
Sources de vérité : sdk/apollia/{agent,skills,react}.py.
"""

from __future__ import annotations

import hashlib
from datetime import datetime
from typing import Any

from apollia import DomainError, agent, react, skill
from apollia.types import Ctx


SYSTEM_PROMPT = """Tu es un agent de résumé technique précis.

Mission : à partir d'une URL fournie, produire un résumé markdown structuré.

Outils :
- `web_read` : fetch + extraction texte d'une URL.
- `file_write` (optionnel) : enregistrer le résumé sur disque.

Règles :
1. Toujours commencer par appeler `web_read` sur l'URL fournie.
2. Si la page est inaccessible, retourner un final_answer court expliquant l'échec.
3. Adapter la profondeur et le vocabulaire au profil utilisateur (stack/langages) si fourni.
4. Format markdown obligatoire :
   ```
   # {Titre déduit}

   **Source :** {URL}
   **Date :** {YYYY-MM-DD}

   ## Résumé
   {3-6 phrases dense}

   ## Points clés
   - {bullet 1}
   - {bullet 2}
   - {bullet 3-7}

   ## Citations notables
   > {1-3 citations courtes}
   ```
5. Final answer = le markdown complet, sans wrapper JSON.
"""


@agent(
    name="markdown-summarizer",
    version="0.1.0",
    description="Résume une URL en markdown adapté au profil utilisateur",
    tags=("summarization", "web", "personal"),
    memory_namespace="markdown-summarizer",
    tools_required=("web_read",),
    step_budget={"max_steps": 12, "max_tool_calls": 6, "wall_clock_secs": 180},
)
class MarkdownSummarizer:
    """L1 standalone — résume une URL en markdown."""

    @skill(
        "markdown.summarize_url",
        description="Prend une URL et retourne un résumé markdown adapté au profil",
    )
    async def summarize_url(
        self,
        url: str,
        max_words: int | None = None,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Résume une URL en markdown structuré."""
        if ctx.llm is None:
            raise DomainError("NO_LLM", "Backend LLM requis pour la synthèse")
        if not url or not url.startswith(("http://", "https://")):
            raise DomainError(
                "NO_URL",
                "URL absente ou invalide.",
                details={"url": url},
            )

        cache_key = f"summary:{_url_hash(url)}"
        if ctx.memory is not None:
            try:
                cached = await ctx.memory.recall(cache_key)
                if cached:
                    ctx.logger.info("cache hit", url=url)
                    return {"markdown": cached, "cached": True, "url": url}
            except Exception as exc:
                ctx.logger.debug("cache lookup failed", error=str(exc))

        user_context = await _load_user_context(ctx)
        extra_lines = [f"URL à résumer : {url}"]
        if user_context.get("user.tech.stack"):
            extra_lines.append(
                f"Stack technique : {user_context['user.tech.stack']} "
                "(adapte la profondeur technique)."
            )
        if user_context.get("user.tech.languages"):
            extra_lines.append(
                f"Langages familiers : {user_context['user.tech.languages']}."
            )
        if max_words:
            extra_lines.append(f"Limite : ~{max_words} mots.")

        tools: list[dict[str, Any]] = []
        if ctx.tools is not None:
            for t in ("web_read", "http_fetch", "file_write"):
                try:
                    tools.append(ctx.tools.describe(t))
                except Exception:
                    pass

        try:
            markdown = await react(
                ctx,
                system=SYSTEM_PROMPT + "\n\n" + "\n".join(extra_lines),
                user=f"URL : {url}",
                tools=tools,
                max_steps=12,
                temperature=0.3,
            )
        except DomainError:
            raise
        except Exception as exc:
            ctx.logger.error("react failed", error=str(exc))
            raise DomainError("REACT_FAILED", str(exc)) from exc

        await _post_run(ctx, url, cache_key, markdown)
        return {"markdown": markdown, "cached": False, "url": url}


def _url_hash(url: str) -> str:
    return hashlib.sha256(url.encode("utf-8")).hexdigest()[:16]


async def _load_user_context(ctx: Ctx) -> dict[str, str]:
    if ctx.profile is None:
        return {}
    out: dict[str, str] = {}
    try:
        entries = await ctx.profile.all()
    except Exception as exc:
        ctx.logger.debug("profile.all() failed", error=str(exc))
        return out
    for key in ("tech.stack", "tech.languages"):
        if entries.get(key):
            out[f"user.{key}"] = entries[key]
    return out


async def _post_run(ctx: Ctx, url: str, cache_key: str, markdown: str) -> None:
    today = datetime.now().strftime("%Y-%m-%d")
    if ctx.memory is not None:
        try:
            await ctx.memory.remember(cache_key, markdown)
            await ctx.memory.record(
                content=f"Résumé généré pour {url} ({len(markdown)} chars)",
                importance=0.5,
                metadata={"url": url, "date": today},
            )
        except Exception as exc:
            ctx.logger.warning("memory persistence failed", error=str(exc))

    if ctx.tools is not None:
        try:
            path = f"~/Documents/markdown-summarizer/{today}-{_url_hash(url)}.md"
            await ctx.tools.call("file_write", {"path": path, "content": markdown})
        except Exception as exc:
            ctx.logger.warning("file_write failed", error=str(exc))

    if ctx.notify is not None:
        try:
            await ctx.notify.publish(
                f"Résumé prêt : {url[:60]}",
                severity="info",
                title="Markdown Summarizer",
            )
        except Exception as exc:
            ctx.logger.debug("notify failed", error=str(exc))

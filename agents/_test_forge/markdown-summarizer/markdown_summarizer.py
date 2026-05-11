"""Markdown Summarizer — résume une URL en markdown adapté au profil utilisateur.

Pattern : L1 standalone (BaseReActAgent).
Sources de vérité : sdk/apollia/agents/react.py + sdk/apollia/stubs/{memory,tools,llm}.py.
"""

from __future__ import annotations

import hashlib
import json
from datetime import datetime
from typing import Any

from apollia.agents.react import AIPResult, BaseReActAgent


class MarkdownSummarizerAgent(BaseReActAgent):
    SYSTEM_PROMPT = """Tu es un agent de résumé technique précis.

Mission : à partir d'une URL fournie, produire un résumé markdown structuré.

Outils :
- `web_read` : fetch + extraction texte d'une URL.
- `file_write` (optionnel) : enregistrer le résumé sur disque.

Règles :
1. Toujours commencer par appeler `web_read` sur l'URL fournie.
2. Si la page est inaccessible, retourner un final_answer court expliquant l'échec.
3. Adapter la profondeur et le vocabulaire au profil utilisateur (stack/langages) si fourni dans le contexte.
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
    MAX_STEPS = 12
    TEMPERATURE = 0.3

    def manifest(self) -> dict[str, Any]:
        return {
            "name": "markdown-summarizer",
            "version": "1.0.0",
            "description": "Résume une URL en markdown adapté au profil utilisateur",
            "execution_mode": "direct",
            "agent_type": "user",
            "tools_required": ["web_read"],
            "tools_optional": ["http_fetch", "file_write", "memory_search"],
            "memory_namespace": "markdown-summarizer",
            "max_concurrent_tasks": 2,
            "supports_a2a": True,
            "user_memory_write": False,
            "dangerous_tools_allowed": False,
            "step_budget": {"max_steps": 12, "max_tool_calls": 6, "wall_clock_secs": 180},
            "tags": ["summarization", "web", "personal"],
            "skills": [
                {
                    "id": "summarize-url",
                    "description": "Prend une URL et retourne un résumé markdown",
                    "input_modes": ["text"],
                    "output_modes": ["text"],
                    "input_schema": {"url": "str", "max_words": "int?"},
                }
            ],
            "setup_notes": "Voir setup.md. Cache mémoire activé par défaut sur clé summary:{hash(url)}.",
            "limitations": "HTML/markdown uniquement (pas de PDF). Mono-URL.",
        }

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        if ctx.llm is None:
            return AIPResult.failed("NO_LLM", "Backend LLM requis pour la synthèse")

        # Extraction input
        parts = task.get("input", {}).get("parts", [])
        user_text = next((p.get("text", "") for p in parts if p.get("type") == "text"), "")
        url = self._extract_url(user_text)
        if not url:
            return AIPResult.failed(
                "NO_URL",
                "Aucune URL détectée dans l'input. Format attendu : URL en clair.",
            )

        # Cache mémoire
        cache_key = f"summary:{self._url_hash(url)}"
        if ctx.memory is not None:
            try:
                cached = await ctx.memory.recall(cache_key)
                if cached:
                    ctx.log("info", f"cache hit for {url}")
                    return AIPResult.completed(cached)
            except Exception as e:
                ctx.log("debug", f"cache lookup failed: {e}")

        # Lecture profil utilisateur (fallback __user__)
        user_context = await self._load_user_context(ctx)
        extra_context_parts = [f"URL à résumer : {url}"]
        if user_context.get("user.tech.stack"):
            extra_context_parts.append(
                f"Stack technique de l'utilisateur : {user_context['user.tech.stack']} "
                "(adapte la profondeur technique en conséquence)."
            )
        if user_context.get("user.tech.languages"):
            extra_context_parts.append(
                f"Langages familiers : {user_context['user.tech.languages']}."
            )
        # Style maison optionnel via APOLLIA.md
        if ctx.workspace is not None:
            try:
                style = ctx.workspace.get("Markdown Summary Style")
                if style:
                    extra_context_parts.append(f"<style_maison>\n{style}\n</style_maison>")
            except Exception as e:
                ctx.log("debug", f"workspace.get failed: {e}")

        extra_context = "\n".join(extra_context_parts)

        # Boucle ReAct
        try:
            result = await self.react(task, ctx, user_text, extra_context=extra_context)
        except Exception as e:
            ctx.log("error", f"react loop failed: {e}")
            return AIPResult.failed("REACT_FAILED", str(e))

        if isinstance(result, dict):
            return result

        # Post-run : cache + episodic + file_write + notify
        await self._post_run(ctx, url, cache_key, result, task)

        return AIPResult.completed(result)

    @staticmethod
    def _extract_url(text: str) -> str | None:
        for token in text.split():
            if token.startswith(("http://", "https://")):
                return token.strip(".,;:)")
        return None

    @staticmethod
    def _url_hash(url: str) -> str:
        return hashlib.sha256(url.encode("utf-8")).hexdigest()[:16]

    async def _load_user_context(self, ctx: Any) -> dict[str, str]:
        # ADR-087 — read the user profile through ctx.profile.
        if ctx.profile is None:
            return {}
        out: dict[str, str] = {}
        try:
            all_entries = await ctx.profile.all()
        except Exception as e:
            ctx.log("debug", f"profile.all() failed: {e}")
            return out
        for key in ("tech.stack", "tech.languages"):
            if all_entries.get(key):
                out[f"user.{key}"] = all_entries[key]
        return out

    async def _post_run(
        self,
        ctx: Any,
        url: str,
        cache_key: str,
        markdown: str,
        task: dict[str, Any],
    ) -> None:
        today = datetime.now().strftime("%Y-%m-%d")

        if ctx.memory is not None:
            try:
                await ctx.memory.remember(cache_key, markdown, source=task.get("task_id"))
                await ctx.memory.record(
                    content=f"Résumé généré pour {url} ({len(markdown)} chars)",
                    importance=0.5,
                    task_id=task.get("task_id"),
                    metadata={"url": url, "date": today},
                )
            except Exception as e:
                ctx.log("warn", f"memory persistence failed: {e}")

        if ctx.tools is not None:
            try:
                path = f"~/Documents/markdown-summarizer/{today}-{self._url_hash(url)}.md"
                await ctx.tools.call("file_write", {"path": path, "content": markdown})
            except Exception as e:
                ctx.log("warn", f"file_write failed: {e}")

        if ctx.notify is not None:
            try:
                await ctx.notify.publish(
                    f"Résumé prêt : {url[:60]}",
                    severity="info",
                    title="Markdown Summarizer",
                )
            except Exception as e:
                ctx.log("debug", f"notify failed: {e}")


# Apollia AIP duck-typed entry point
agent = MarkdownSummarizerAgent()


def manifest() -> dict[str, Any]:
    return agent.manifest()


async def run(task: dict[str, Any], ctx: Any) -> dict[str, Any]:
    return await agent.run(task, ctx)

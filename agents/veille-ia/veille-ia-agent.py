"""veille-ia-agent — Director Agent for daily AI/LLM watch.

Orchestrates a daily intelligence briefing on two axes:
- Technical: new models, frameworks, tools, research breakthroughs
- Competitive: news about n8n, Make, Zapier AI, Lindy AI, Dust.tt, etc.

Architecture (Director + 2 Workers via A2A):

  veille-ia-agent  (this file)
  ├── web-search-worker   skill: "search-and-extract"
  │     Receives: queries, axis, seen_hashes
  │     Returns:  articles list with deduplication
  └── synthesis-worker    skill: "synthesize-report"
        Receives: all articles, date, axes definitions
        Returns:  Markdown report + summary + top items

Memory design (namespace "veille-ia"):
  Semantic:
    "bootstrap.snapshot"     → {competitors, tech_terms, competitive_terms, axes}
    "bootstrap.status"       → "complete" | "missing"
    "bootstrap.meta"         → {created_at, version}
    "seen:{hash}"            → {title, url, date_seen}  ← cross-session dedup
    "last_run_date"          → ISO date
    "total_runs"             → int
  Episodic:
    one record per run with importance 0.7

Required tools: file_write
Optional tools: file_list
Required A2A skills: search-and-extract (web-search-worker)
                     synthesize-report  (synthesis-worker)
"""

from __future__ import annotations

import json
import os
from datetime import date, datetime
from typing import Any

from apollia.agents import AIPResult, BaseReActAgent

# ---------------------------------------------------------------------------
# Bootstrap helpers (inline — no SDK bootstrap module needed)
# ---------------------------------------------------------------------------

_BOOTSTRAP_KEY_SNAPSHOT = "bootstrap.snapshot"
_BOOTSTRAP_KEY_STATUS = "bootstrap.status"
_BOOTSTRAP_KEY_META = "bootstrap.meta"
_BOOTSTRAP_TTL_DAYS = 7

_INITIAL_SNAPSHOT: dict[str, Any] = {
    "competitors": [
        "n8n", "Make", "Zapier AI", "Relay.app", "Lindy AI",
        "Dust.tt", "Beam AI", "Cognosys", "Lutra AI", "Replit Agent",
        "OpenClaw", "Claude Cowork", "Microsoft Copilot Studio",
        "Office 365 Copilot", "Google Agentspace",
    ],
    "tech_queries": [
        "new LLM model release 2026",
        "AI agent framework news",
        "Anthropic Claude news",
        "OpenAI GPT news",
        "Google Gemini news",
        "MCP Model Context Protocol news",
        "Agent-to-Agent protocol A2A",
        "RAG retrieval augmented generation",
        "LangGraph CrewAI AutoGen update",
    ],
    "competitive_queries": [
        "n8n AI agent automation news",
        "Make Integromat AI workflow",
        "Zapier AI agents update",
        "AI workflow automation startup funding",
        "no-code AI agent platform launch",
        "enterprise AI assistant product launch",
    ],
    "axes": {
        "tech": "Actualités techniques IA/LLM (modèles, frameworks, outils, recherche)",
        "competitive": "Actualités concurrentielles (produits, funding, lancements)",
    },
}


async def _needs_bootstrap(ctx: Any) -> bool:
    """Return True if the competitive landscape snapshot is missing or stale."""
    if ctx.memory is None:
        return False
    status = await ctx.memory.recall(_BOOTSTRAP_KEY_STATUS)
    if status != "complete":
        return True
    meta_raw = await ctx.memory.recall(_BOOTSTRAP_KEY_META)
    if not meta_raw:
        return True
    try:
        meta = json.loads(meta_raw)
        created = datetime.fromisoformat(meta.get("created_at", "2000-01-01"))
        return (datetime.now() - created).days > _BOOTSTRAP_TTL_DAYS
    except Exception:
        return True


async def _run_bootstrap(ctx: Any) -> dict[str, Any]:
    """Persist the initial competitive landscape to memory and return it."""
    if ctx.memory is None:
        return _INITIAL_SNAPSHOT
    snapshot = _INITIAL_SNAPSHOT.copy()
    await ctx.memory.remember(
        _BOOTSTRAP_KEY_SNAPSHOT,
        json.dumps(snapshot, ensure_ascii=False),
        source="veille-ia-agent",
        confidence=1.0,
    )
    await ctx.memory.remember(
        _BOOTSTRAP_KEY_META,
        json.dumps({"created_at": datetime.now().isoformat(), "version": "1.0"}),
        source="veille-ia-agent",
    )
    await ctx.memory.remember(
        _BOOTSTRAP_KEY_STATUS,
        "complete",
        source="veille-ia-agent",
    )
    await ctx.memory.record(
        "Bootstrap du paysage concurrentiel effectué",
        importance=0.5,
    )
    return snapshot


async def _load_snapshot(ctx: Any) -> dict[str, Any]:
    """Load the bootstrap snapshot from memory, falling back to defaults."""
    if ctx.memory is None:
        return _INITIAL_SNAPSHOT
    raw = await ctx.memory.recall(_BOOTSTRAP_KEY_SNAPSHOT)
    if not raw:
        return _INITIAL_SNAPSHOT
    try:
        return json.loads(raw)
    except Exception:
        return _INITIAL_SNAPSHOT


async def _collect_seen_hashes(ctx: Any, limit: int = 500) -> list[str]:
    """Return URL hashes seen in previous runs (for deduplication)."""
    if ctx.memory is None:
        return []
    try:
        # Search for all "seen:" keys in memory
        results = await ctx.memory.search("seen:", limit=limit)
        hashes = []
        for r in results:
            content = r.get("content", "")
            if content.startswith("seen:"):
                hashes.append(content.split(":", 1)[1][:12])
        return hashes
    except Exception:
        return []


async def _persist_seen_articles(ctx: Any, articles: list[dict], run_date: str) -> None:
    """Store article URL hashes to memory for cross-session deduplication."""
    if ctx.memory is None:
        return
    for article in articles:
        url_hash = article.get("hash", "")
        if not url_hash:
            continue
        await ctx.memory.remember(
            f"seen:{url_hash}",
            json.dumps({
                "title": article.get("title", ""),
                "url": article.get("url", ""),
                "date_seen": run_date,
            }),
            source="veille-ia-agent",
            confidence=1.0,
        )


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------

SYSTEM_PROMPT: str = """\
Tu es veille-ia-agent, un agent directeur de veille IA/LLM pour les équipes \
d'Apollia OS.

## MISSION

Chaque jour, tu orchestre une veille sur deux axes :
- **Technique** : nouveaux modèles, frameworks, outils, avancées recherche
- **Concurrentiel** : news sur les concurrents (n8n, Make, Zapier AI, etc.)

## COMMENT TU TRAVAILLES

Tu es un directeur : tu ne fais pas de recherche toi-même. Tu délègues :

1. `a2a:search-and-extract` (web-search-worker) — pour chaque axe, envoie les
   requêtes et récupère les articles dédupliqués

2. `a2a:synthesize-report` (synthesis-worker) — envoie tous les articles pour
   produire le rapport Markdown final

3. `file_write` — sauvegarde le rapport dans ~/.apollia/reports/veille-{date}.md

## ORCHESTRATION STEP BY STEP

Étape 1 — Appelle a2a:search-and-extract pour l'axe tech
Étape 2 — Appelle a2a:search-and-extract pour l'axe concurrentiel
Étape 3 — Fusionne les articles des 2 axes
Étape 4 — Appelle a2a:synthesize-report avec tous les articles
Étape 5 — Sauvegarde le rapport avec file_write
Étape 6 — Retourne final_answer avec le résumé et le chemin du rapport

## GESTION DES ERREURS

- Si search-and-extract échoue sur un axe : continuer avec 0 articles pour cet axe
- Si synthesize-report échoue : construire un rapport minimal de secours
- Si file_write échoue : retourner le rapport en texte dans final_answer

## FORMAT final_answer

Quand tout est terminé :
"Veille du {date} générée : {N} articles analysés. Rapport sauvegardé : {path}

Top items :
- {top1}
- {top2}
- {top3}"
"""

# ---------------------------------------------------------------------------
# Manifest
# ---------------------------------------------------------------------------


def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for veille-ia-agent."""
    return {
        "name": "veille-ia-agent",
        "version": "1.0.0",
        "description": (
            "Agent directeur de veille quotidienne IA/LLM. "
            "Orchestre la collecte (web-search-worker via A2A), la synthèse "
            "(synthesis-worker via A2A), et la sauvegarde du rapport Markdown. "
            "Maintient une mémoire cross-session pour la déduplication des articles "
            "et le suivi du paysage concurrentiel."
        ),
        "execution_mode": "direct",
        "agent_type": "assistant",
        "tools_required": ["file_write"],
        "tools_optional": [
            "file_list",
            "a2a:search-and-extract",
            "a2a:synthesize-report",
        ],
        "tools_requiring_approval": [],
        "packages": [],
        "memory_namespace": "veille-ia",
        "supports_streaming": False,
        "supports_a2a": True,
        "step_budget": {
            "max_steps": 20,
            "max_tool_calls": 15,
            "wall_clock_secs": 1200,
        },
        "tags": ["watch", "research", "daily", "director", "a2a-orchestrator"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
        "skills": [
            {
                "id": "run-daily-watch",
                "name": "Lancer la veille quotidienne",
                "description": (
                    "Exécute un cycle complet de veille IA/LLM : recherche, "
                    "déduplication, synthèse, sauvegarde rapport. "
                    "Retourne le chemin du rapport et le résumé exécutif."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
            }
        ],
        "examples": [
            "Génère la veille IA du jour",
            "Lance la veille IA/LLM",
            "Quelles sont les news IA d'aujourd'hui ?",
        ],
        "limitations": [
            "Nécessite web-search-worker et synthesis-worker actifs pour fonctionner.",
            "Les outils web doivent être activés dans apollia.toml ([tools.web] enabled = true).",
            "En cas d'absence de workers, retourne un rapport minimal basé sur la mémoire.",
        ],
        "setup_notes": (
            "Activer [tools.web] dans apollia.toml avant de démarrer. "
            "Démarrer web-search-worker et synthesis-worker au préalable. "
            "Le premier run bootstrappe automatiquement le paysage concurrentiel en mémoire."
        ),
    }


# ---------------------------------------------------------------------------
# Director agent
# ---------------------------------------------------------------------------


class VeilleIaAgent(BaseReActAgent):
    """Director Agent for daily AI/LLM intelligence watch.

    Lifecycle per run:
    1. Bootstrap competitive landscape to memory (once, TTL 7 days)
    2. Collect seen URL hashes (cross-session dedup)
    3. Delegate web search to web-search-worker (tech axis)
    4. Delegate web search to web-search-worker (competitive axis)
    5. Delegate synthesis to synthesis-worker
    6. Persist report to ~/.apollia/reports/
    7. Update memory: seen hashes, last_run_date, total_runs, episodic record
    """

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 20
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Execute one full watch cycle."""
        if ctx.llm is None:
            return AIPResult.failed(
                "NO_LLM",
                "veille-ia-agent requires ctx.llm — no LLM backend configured.",
            )

        today = date.today().isoformat()

        # --- Bootstrap competitive landscape ---
        if await _needs_bootstrap(ctx):
            snapshot = await _run_bootstrap(ctx)
        else:
            snapshot = await _load_snapshot(ctx)

        # --- Collect seen hashes for deduplication ---
        seen_hashes = await _collect_seen_hashes(ctx)

        # --- Update total_runs counter ---
        if ctx.memory is not None:
            runs_raw = await ctx.memory.recall("total_runs")
            try:
                total_runs = int(runs_raw or "0") + 1
            except ValueError:
                total_runs = 1
            await ctx.memory.remember("total_runs", str(total_runs), source="veille-ia-agent")

        # --- Build user message for the ReAct loop ---
        user_message = self._build_user_message(today, snapshot, seen_hashes)

        result = await self.react(task, ctx, user_message)
        if isinstance(result, dict):
            # On failure, still record the attempt
            await self._record_run(ctx, today, 0, success=False)
            return result

        # Extract article count and report path from the final answer text
        article_count = self._count_articles_in_text(result)
        report_path = self._extract_report_path(result, today)

        # --- Update memory after successful run ---
        await self._update_memory_post_run(ctx, today, result)
        await self._record_run(ctx, today, article_count, success=True)

        return AIPResult.completed(result)

    def _build_user_message(
        self,
        today: str,
        snapshot: dict[str, Any],
        seen_hashes: list[str],
    ) -> str:
        tech_queries = snapshot.get("tech_queries", _INITIAL_SNAPSHOT["tech_queries"])
        competitive_queries = snapshot.get("competitive_queries", _INITIAL_SNAPSHOT["competitive_queries"])
        axes = snapshot.get("axes", _INITIAL_SNAPSHOT["axes"])
        report_dir = os.path.expanduser("~/.apollia/reports")
        report_path = f"{report_dir}/veille-{today}.md"

        return (
            f"Lance la veille quotidienne du {today}.\n\n"
            f"**Requêtes tech ({len(tech_queries)}) :**\n"
            + json.dumps(tech_queries, ensure_ascii=False)
            + f"\n\n**Requêtes concurrentielles ({len(competitive_queries)}) :**\n"
            + json.dumps(competitive_queries, ensure_ascii=False)
            + f"\n\n**Hashes déjà vus ({len(seen_hashes)}) :** présents dans la mémoire\n"
            f"Passes-les aux workers pour éviter les doublons : "
            + json.dumps(seen_hashes[:50])  # Limit to 50 to avoid huge payloads
            + f"\n\n**Chemin du rapport à sauvegarder :** `{report_path}`\n\n"
            "Orchestre les étapes dans l'ordre : search tech → search concurrentiel "
            "→ synthesize → file_write → final_answer."
        )

    async def _update_memory_post_run(
        self,
        ctx: Any,
        today: str,
        result_text: str,
    ) -> None:
        """Update last_run_date. Article hashes are persisted by react() via tools."""
        if ctx.memory is None:
            return
        await ctx.memory.remember(
            "last_run_date",
            today,
            source="veille-ia-agent",
            confidence=1.0,
        )

    async def _record_run(
        self,
        ctx: Any,
        today: str,
        article_count: int,
        success: bool,
    ) -> None:
        """Write an episodic memory record for this run."""
        if ctx.memory is None:
            return
        status = "succès" if success else "échec"
        await ctx.memory.record(
            f"Run du {today} : {article_count} articles, {status}",
            importance=0.7,
        )

    @staticmethod
    def _count_articles_in_text(text: str) -> int:
        """Try to extract article count from the final answer text."""
        import re
        match = re.search(r"(\d+)\s+article", text)
        return int(match.group(1)) if match else 0

    @staticmethod
    def _extract_report_path(text: str, today: str) -> str:
        """Extract report path from final answer or return default."""
        import re
        match = re.search(r"(~?/[^\s]+veille[^\s]+\.md)", text)
        if match:
            return match.group(1)
        return f"~/.apollia/reports/veille-{today}.md"


agent = VeilleIaAgent()

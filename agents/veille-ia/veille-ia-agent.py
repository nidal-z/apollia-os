"""veille-ia-agent — Director Agent for daily AI/LLM watch.

Orchestrates a daily intelligence briefing on two axes :
- **Technical** : new models, frameworks, tools, research breakthroughs.
- **Competitive** : news on n8n, Make, Zapier AI, Lindy AI, Dust.tt, etc.

## Architecture (Director + 2 Workers via A2A)

  veille-ia-agent  (this file — director)
  ├── web-search-worker   skill: ``search-and-extract``
  │     Receives: queries, axis, seen_hashes
  │     Returns:  articles list with deduplication
  └── synthesis-worker    skill: ``synthesize-report``
        Receives: all articles, date, axes definitions
        Returns:  Markdown report + summary + top items

The director never executes the search itself — it delegates. As a fallback
when A2A is unavailable, it can call ``web_search`` / ``web_read`` directly.

## Memory (namespace ``veille-ia``)

Semantic :
- ``bootstrap.snapshot``  — competitive landscape (refreshed every 7 days)
- ``seen:{hash}``         — URL hashes seen in previous runs (cross-session dedup)
- ``last_run_date``       — ISO date
- ``total_runs``          — int

Episodic :
- one record per run with importance 0.7

## Design notes (v2.0.0 — refonte)

The earlier versions wrapped the LLM in heuristic guardrails (anti-
hallucination gate, mandatory tool-call counts, file-mtime checks,
preflight tool availability checks). They created more failure modes
than they prevented and were antithetical to ADR-021's *agent at the
helm* doctrine. v2.0.0 trusts the LLM and the runtime :

- All research tools are in ``tools_required`` — fail-fast at startup
  if missing (Principe #4) instead of probing at runtime.
- The system prompt describes the goal, not policed behavior.
- Anti-hallucination gate removed — output quality is judged by the
  operator (or by post-hoc memory analysis), not by run-time rejection.
- Memory dedup via ``seen:{hash}`` is the single hard contract kept,
  because it's what makes daily runs progressive.
"""

from __future__ import annotations

import hashlib
import json
import re
from datetime import date, datetime
from typing import Any
from urllib.parse import urlparse

from apollia.agents import AIPResult, BaseReActAgent

# ---------------------------------------------------------------------------
# Bootstrap helpers — competitive landscape persisted to memory
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
        json.dumps({"created_at": datetime.now().isoformat(), "version": "2.0"}),
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
        results = await ctx.memory.search("seen:", limit=limit)
        hashes: list[str] = []
        for r in results:
            content = r.get("content", "")
            if content.startswith("seen:"):
                hashes.append(content.split(":", 1)[1][:12])
        return hashes
    except Exception:
        return []


# ---------------------------------------------------------------------------
# Side effects — desktop notification, dual-write to ~/Documents
# ---------------------------------------------------------------------------


async def _notify_completion(ctx: Any, today: str, article_count: int) -> None:
    """Emit a desktop notification at end of run."""
    if ctx.notify is None:
        return
    try:
        await ctx.notify.publish(
            f"Veille IA du {today} : {article_count} articles",
            severity="info",
            title="Apollia — Veille IA",
        )
    except Exception as e:
        ctx.log("debug", f"notify failed: {e}")


async def _dual_write_report(ctx: Any, today: str, content: str) -> None:
    """Mirror the report into ~/Documents/veille-ia/ for easy operator access."""
    if ctx.tools is None:
        return
    try:
        path = f"~/Documents/veille-ia/{today}.md"
        await ctx.tools.call("file_write", {"path": path, "content": content})
    except Exception as e:
        ctx.log("debug", f"dual file_write failed: {e}")


# ---------------------------------------------------------------------------
# Memory persistence — seen URLs for cross-day dedup
# ---------------------------------------------------------------------------

# Markdown link `[title](url)` — preferred citation format.
_MD_LINK_RE = re.compile(r"\[([^\]]+)\]\((https?://[^\s)]+)\)")
# Bare URL fallback — captures URLs the LLM didn't wrap in markdown.
_BARE_URL_RE = re.compile(r"https?://[^\s)\]]+")


def _extract_urls(report_text: str) -> list[dict[str, str]]:
    """Pull (title, url, source) tuples from the LLM report.

    Best-effort harvest for memory dedup — *never* used to validate or
    reject the run. Returns deduped entries by URL hash.
    """
    seen: set[str] = set()
    out: list[dict[str, str]] = []
    for m in _MD_LINK_RE.finditer(report_text):
        title, url = m.group(1).strip(), m.group(2).strip().rstrip(".,;)")
        if url in seen:
            continue
        seen.add(url)
        out.append({"title": title, "url": url, "source": _hostname(url)})
    for m in _BARE_URL_RE.finditer(report_text):
        url = m.group(0).strip().rstrip(".,;)")
        if url in seen:
            continue
        seen.add(url)
        out.append({"title": "", "url": url, "source": _hostname(url)})
    return out


def _hostname(url: str) -> str:
    try:
        return (urlparse(url).hostname or "").removeprefix("www.")
    except Exception:
        return ""


async def _persist_seen_articles(
    ctx: Any, articles: list[dict[str, str]], run_date: str
) -> int:
    """Persist URL hashes for cross-day dedup. Returns new entries count."""
    if ctx.memory is None:
        return 0
    new_count = 0
    for article in articles:
        url = article.get("url", "")
        if not url:
            continue
        url_hash = hashlib.sha256(url.encode()).hexdigest()[:12]
        if await ctx.memory.recall(f"seen:{url_hash}"):
            continue
        await ctx.memory.remember(
            f"seen:{url_hash}",
            json.dumps({
                "title": article.get("title", ""),
                "url": url,
                "source": article.get("source", ""),
                "date_seen": run_date,
            }),
            source="veille-ia-agent",
            confidence=1.0,
        )
        new_count += 1
    return new_count


# ---------------------------------------------------------------------------
# System prompt — descriptive, not policed
# ---------------------------------------------------------------------------

SYSTEM_PROMPT: str = """\
Tu es veille-ia-agent, l'agent directeur de la veille IA/LLM quotidienne
d'Apollia OS.

## Mission

Chaque jour, produire une note de veille structurée sur deux axes :
- **Technique** : nouveaux modèles, frameworks, outils, avancées recherche.
- **Concurrentiel** : actualités sur n8n, Make, Zapier AI, Lindy AI,
  Dust.tt et les autres acteurs du marché des agents.

## Délégation A2A — chemin préféré

Tu disposes de deux workers spécialisés. Privilégie-les :

1. ``a2a:search-and-extract`` (web-search-worker) — un appel par axe.
   Passe-lui les requêtes et la liste des `seen_hashes` (URLs déjà
   couvertes par les runs précédents, fournie dans le user message).
2. ``a2a:synthesize-report`` (synthesis-worker) — un appel avec tous
   les articles fusionnés des deux axes pour obtenir le rapport
   Markdown final.
3. ``file_write`` — sauvegarde le rapport au chemin fourni.
4. ``final_answer`` — résumé exécutif (3 top items + chemin du rapport).

## Recherche directe — fallback

Si un appel A2A échoue ou retourne `unknown skill`, fais-toi la
recherche toi-même avec ``web_search`` et ``web_read`` (au moins 2
requêtes par axe, lis 3-4 articles), puis synthétise ton rapport.
N'abandonne pas la mission : produis le meilleur rapport possible avec
les outils disponibles.

## Format de rapport

Markdown, deux sections « Technique » et « Concurrentiel ». Chaque
entrée cite sa source sous la forme `[titre](url)` quand l'URL est
disponible — c'est ce qui permet à la mémoire de te montrer les
doublons au prochain run.

Termine par `## Top items` listant 3 nouveautés saillantes.
"""

# ---------------------------------------------------------------------------
# Manifest
# ---------------------------------------------------------------------------


def manifest() -> dict[str, Any]:
    return {
        "name": "veille-ia-agent",
        "version": "2.0.0",
        "description": (
            "Agent directeur de veille quotidienne IA/LLM. Délègue la "
            "collecte (web-search-worker via A2A) et la synthèse "
            "(synthesis-worker via A2A) à deux workers spécialisés ; "
            "fallback en recherche directe si l'A2A est indisponible. "
            "Persiste les URLs vues pour la déduplication cross-jour."
        ),
        "execution_mode": "direct",
        "agent_type": "assistant",
        # Outils requis pour le fallback direct — fail-fast au démarrage.
        "tools_required": ["web_search", "web_read", "file_write"],
        "tools_optional": [
            "file_read",
            "file_list",
            "memory_search",
            "a2a:search-and-extract",
            "a2a:synthesize-report",
        ],
        "tools_requiring_approval": [],
        "packages": [],
        "memory_namespace": "veille-ia",
        "supports_streaming": False,
        "supports_a2a": True,
        "step_budget": {
            "max_steps": 25,
            "max_tool_calls": 20,
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
            "Fonctionne mieux quand web-search-worker et synthesis-worker "
            "sont actifs (mode A2A) ; sinon recherche directe.",
            "Les outils web (web_search, web_read) doivent être activés "
            "globalement (Réglages → Outils).",
        ],
        "setup_notes": (
            "Activer web_search et web_read dans Réglages → Outils. "
            "Démarrer les deux workers du package en même temps que le "
            "director pour bénéficier du chemin A2A préféré."
        ),
    }


# ---------------------------------------------------------------------------
# Director agent
# ---------------------------------------------------------------------------


class VeilleIaAgent(BaseReActAgent):
    """Director Agent for daily AI/LLM intelligence watch.

    v2.0.0 refonte — agent in the helm. The director trusts its workers
    and the LLM ; no anti-hallucination gates, no preflight checks. The
    only post-processing is the persistence of cited URLs into memory
    so tomorrow's run can avoid duplicates.
    """

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 25
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        if ctx.llm is None:
            return AIPResult.failed(
                "NO_LLM",
                "veille-ia-agent requires ctx.llm — no LLM backend configured.",
            )

        today = date.today().isoformat()

        # 1. Bootstrap competitive landscape (refresh every 7 days).
        if await _needs_bootstrap(ctx):
            snapshot = await _run_bootstrap(ctx)
        else:
            snapshot = await _load_snapshot(ctx)

        # 2. Cross-day URL dedup — fed to the worker via the user message.
        seen_hashes = await _collect_seen_hashes(ctx)

        # 3. Bump total_runs counter (purely informational).
        if ctx.memory is not None:
            runs_raw = await ctx.memory.recall("total_runs")
            try:
                total_runs = int(runs_raw or "0") + 1
            except ValueError:
                total_runs = 1
            await ctx.memory.remember(
                "total_runs", str(total_runs), source="veille-ia-agent",
            )

        # 4. Run the ReAct loop.
        user_message = self._build_user_message(today, snapshot, seen_hashes)
        result = await self.react(task, ctx, user_message)

        # 5. Failure path — record the attempt for episodic memory.
        if isinstance(result, dict):
            await self._record_run(ctx, today, 0, success=False)
            return result

        # 6. Success path — persist seen URLs (best-effort), notify, dual-write.
        articles = _extract_urls(result)
        new_count = await _persist_seen_articles(ctx, articles, today)
        await self._record_run(
            ctx, today, len(articles), success=True, new_count=new_count,
        )
        await _dual_write_report(ctx, today, result)
        await _notify_completion(ctx, today, len(articles))

        if ctx.memory is not None:
            await ctx.memory.remember(
                "last_run_date", today, source="veille-ia-agent", confidence=1.0,
            )

        return AIPResult.completed(result)

    def _build_user_message(
        self,
        today: str,
        snapshot: dict[str, Any],
        seen_hashes: list[str],
    ) -> str:
        tech_queries = snapshot.get("tech_queries", _INITIAL_SNAPSHOT["tech_queries"])
        competitive_queries = snapshot.get(
            "competitive_queries", _INITIAL_SNAPSHOT["competitive_queries"],
        )
        report_path = f"~/.apollia/reports/veille-{today}.md"

        return (
            f"Lance la veille du {today}.\n\n"
            f"**Requêtes tech ({len(tech_queries)}) :**\n"
            f"{json.dumps(tech_queries, ensure_ascii=False)}\n\n"
            f"**Requêtes concurrentielles ({len(competitive_queries)}) :**\n"
            f"{json.dumps(competitive_queries, ensure_ascii=False)}\n\n"
            f"**URLs déjà couvertes par les runs précédents "
            f"({len(seen_hashes)} hashes 12-chars)** — passe-les aux workers "
            f"pour qu'ils sautent les doublons :\n"
            f"{json.dumps(seen_hashes[:50])}\n\n"
            f"**Chemin de sauvegarde du rapport :** `{report_path}`"
        )

    async def _record_run(
        self,
        ctx: Any,
        today: str,
        article_count: int,
        success: bool,
        new_count: int = 0,
    ) -> None:
        if ctx.memory is None:
            return
        if success:
            note = (
                f"Run du {today} : {article_count} articles "
                f"({new_count} nouveaux), succès"
            )
        else:
            note = f"Run du {today} : échec (aucun article retenu)"
        await ctx.memory.record(note, importance=0.7)


agent = VeilleIaAgent()

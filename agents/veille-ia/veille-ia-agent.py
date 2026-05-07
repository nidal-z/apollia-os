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

import hashlib
import json
import os
import re
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


# --- v1.1.0 additions: user profile, procedure memory, notifications, dual write ---

_USER_KEYS_OF_INTEREST = (
    "user.tools.daily",
    "user.domain.sector",
    "user.tech.proficiency",
    # Legacy keys kept as fallback for users onboarded before the Profile refactor.
    "user.tech.stack",
    "user.tech.languages",
)
_PROCEDURE_TRIGGER = "daily-veille-ia"
_PROCEDURE_STEPS = [
    "Charger snapshot bootstrap (TTL 7j)",
    "Collecter seen:{hash} pour dédup cross-run",
    "Lire profil utilisateur (user.tools.daily, user.domain.sector)",
    "Déléguer a2a:search-and-extract pour axe tech",
    "Déléguer a2a:search-and-extract pour axe concurrentiel",
    "Fusionner articles, déléguer a2a:synthesize-report",
    "Écrire rapport dans ~/.apollia/reports/ ET ~/Documents/veille-ia/",
    "Persister last_run_date, total_runs, episodic record",
    "Notifier desktop fin de run",
]


async def _load_user_context(ctx: Any) -> dict[str, str]:
    """Lit les clés user.* utiles pour personnaliser la veille (v1.1.0)."""
    if ctx.memory is None:
        return {}
    out: dict[str, str] = {}
    for key in _USER_KEYS_OF_INTEREST:
        try:
            value = await ctx.memory.recall(key)
            if value:
                out[key] = value
        except Exception as e:
            ctx.log("debug", f"recall {key} failed: {e}")
    return out


async def _ensure_procedure(ctx: Any) -> None:
    """Enregistre la procédure de veille en mémoire procédurale si absente (v1.1.0)."""
    if ctx.memory is None:
        return
    try:
        existing = await ctx.memory.recall_procedure(_PROCEDURE_TRIGGER)
        if existing:
            return
        await ctx.memory.learn_procedure(_PROCEDURE_TRIGGER, _PROCEDURE_STEPS)
    except Exception as e:
        ctx.log("debug", f"learn_procedure failed: {e}")


async def _notify_completion(ctx: Any, today: str, article_count: int) -> None:
    """Publie une notification desktop en fin de run (v1.1.0)."""
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


async def _dual_write_report(ctx: Any, today: str, report_path_default: str, content: str) -> None:
    """Écrit le rapport dans ~/Documents/veille-ia/ en plus du chemin canonique (v1.1.0).

    Le chemin canonique ~/.apollia/reports/ est géré par le ReAct loop via file_write.
    Cette méthode AJOUTE une copie dans le dossier utilisateur sans toucher au flow existant.
    """
    if ctx.tools is None:
        return
    try:
        path = f"~/Documents/veille-ia/{today}.md"
        await ctx.tools.call("file_write", {"path": path, "content": content})
    except Exception as e:
        ctx.log("debug", f"dual file_write failed: {e}")


async def _persist_seen_articles(ctx: Any, articles: list[dict], run_date: str) -> int:
    """Store article URL hashes to memory for cross-session deduplication.

    Returns the number of articles newly persisted (not already seen).
    """
    if ctx.memory is None:
        return 0
    new_count = 0
    for article in articles:
        url = article.get("url", "")
        url_hash = article.get("hash") or (
            hashlib.sha256(url.encode()).hexdigest()[:12] if url else ""
        )
        if not url_hash:
            continue
        existing = await ctx.memory.recall(f"seen:{url_hash}")
        if existing:
            continue
        await ctx.memory.remember(
            f"seen:{url_hash}",
            json.dumps({
                "title": article.get("title", ""),
                "url": url,
                "source": article.get("source", ""),
                "axis": article.get("axis", ""),
                "date_seen": run_date,
            }),
            source="veille-ia-agent",
            confidence=1.0,
        )
        new_count += 1
    return new_count


# ---------------------------------------------------------------------------
# Result parsing & validation
# ---------------------------------------------------------------------------

# Markdown link pattern: [title](url) — captures title + URL.
_MD_LINK_RE = re.compile(r"\[([^\]]+)\]\((https?://[^\s)]+)\)")
# Bare URL pattern as a fallback when the LLM doesn't use markdown links.
_BARE_URL_RE = re.compile(r"https?://[^\s)\]]+")


def _extract_articles_from_report(report_text: str) -> list[dict]:
    """Pull (title, url, source) tuples from a Markdown report.

    Uses two passes:
    1. Markdown links `[title](url)` — preferred, gives both title and URL.
    2. Bare URLs without markdown wrapping — fallback (no title).

    Returns deduped articles by URL hash.
    """
    seen_urls: set[str] = set()
    out: list[dict] = []
    for m in _MD_LINK_RE.finditer(report_text):
        title, url = m.group(1).strip(), m.group(2).strip()
        if url in seen_urls:
            continue
        seen_urls.add(url)
        host = ""
        try:
            from urllib.parse import urlparse

            host = (urlparse(url).hostname or "").removeprefix("www.")
        except Exception:
            pass
        out.append({
            "title": title,
            "url": url,
            "source": host,
            "hash": hashlib.sha256(url.encode()).hexdigest()[:12],
        })
    for m in _BARE_URL_RE.finditer(report_text):
        url = m.group(0).strip().rstrip(".,;")
        if url in seen_urls:
            continue
        seen_urls.add(url)
        host = ""
        try:
            from urllib.parse import urlparse

            host = (urlparse(url).hostname or "").removeprefix("www.")
        except Exception:
            pass
        out.append({
            "title": "",
            "url": url,
            "source": host,
            "hash": hashlib.sha256(url.encode()).hexdigest()[:12],
        })
    return out


# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------

SYSTEM_PROMPT: str = """\
Tu es veille-ia-agent, un agent directeur de veille IA/LLM pour les équipes \
d'Apollia OS.

## MISSION

Chaque jour, tu produis une veille fondée sur des **sources réelles**, jamais
inventées. Deux axes :
- **Technique** : nouveaux modèles, frameworks, outils, avancées recherche.
- **Concurrentiel** : news sur les concurrents (n8n, Make, Zapier AI, etc.).

## RÈGLE ABSOLUE — INTERDICTION D'HALLUCINER

Tu n'as PAS le droit d'inventer des articles, des titres ou des sources. Toute
information du rapport doit provenir d'un appel `web_search` ou
`a2a:search-and-extract` que TU as réellement effectué dans cette session.

Si tu produis un `final_answer` sans avoir effectué au moins **4 appels
`web_search` réussis** (ou 2 `a2a:search-and-extract` réussis), le runtime
rejettera le run comme invalide.

Symptôme à éviter : "Lancement du Qwen-3 — Mise à jour AgentOS 2.0 —
Nouvelles fonctionnalités Zapier" sortis sans le moindre appel d'outil.
C'est une hallucination — refuse de la produire.

## COMMENT TU TRAVAILLES — DEUX CHEMINS

### Chemin préféré — délégation A2A

1. `a2a:search-and-extract` (web-search-worker) — un appel par axe avec les
   requêtes et `seen_hashes` pour dédup.
2. `a2a:synthesize-report` (synthesis-worker) — un appel avec tous les
   articles pour produire le rapport Markdown.
3. `file_write` — sauvegarde du rapport dans le chemin fourni.
4. `final_answer` avec le résumé.

### Chemin de secours — recherche directe

Si `a2a:search-and-extract` retourne « unknown skill » ou échoue, OU si tu
n'as aucun worker A2A disponible, tu DOIS faire la recherche toi-même :

1. Au moins **2 appels `web_search`** pour l'axe tech (requêtes différentes).
2. Au moins **2 appels `web_search`** pour l'axe concurrentiel.
3. Pour chaque résultat pertinent, **`web_read`** pour récupérer le contenu
   réel (au moins 4 lectures au total).
4. Synthèse manuelle du rapport Markdown (sections "Tech" / "Concurrentiel" /
   "Top items"), uniquement à partir des articles que tu as réellement lus.
   Chaque entrée doit citer son URL source.
5. `file_write` pour sauvegarder.
6. `final_answer` avec le résumé.

## INTERDIT

- `final_answer` directement après seulement `file_write` (pas de recherche).
- `final_answer` annonçant un échec ("Mandatory tools unavailable", "Cannot
  generate report"…) sans avoir tenté le chemin de secours.
- Inventer un titre, une source, une URL ou une statistique.

## FORMAT final_answer

Quand tout est terminé :
"Veille du {date} générée : {N} articles analysés (dont {K} nouveaux).
Rapport sauvegardé : {path}

Top items :
- {titre1} — {source1}
- {titre2} — {source2}
- {titre3} — {source3}"
"""

# ---------------------------------------------------------------------------
# Manifest
# ---------------------------------------------------------------------------


def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for veille-ia-agent."""
    return {
        "name": "veille-ia-agent",
        "version": "1.4.0",
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
            "file_read",
            "file_list",
            "web_search",
            "web_read",
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
        run_started_at = datetime.now()

        # --- Pre-flight: confirm research tools are actually available ---
        # If the runtime registry can't describe web_search, the LLM will
        # see no schema for it and bail to file_write hallucinations. Surface
        # this as a clear diagnostic, not a silent generic failure.
        await self._check_research_tools(ctx)

        # --- Bootstrap competitive landscape ---
        if await _needs_bootstrap(ctx):
            snapshot = await _run_bootstrap(ctx)
        else:
            snapshot = await _load_snapshot(ctx)

        # --- v1.1.0: ensure procedure memory + load user profile ---
        await _ensure_procedure(ctx)
        user_context = await _load_user_context(ctx)

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

        # --- Build user message for the ReAct loop (v1.1.0: include user_context) ---
        user_message = self._build_user_message(today, snapshot, seen_hashes, user_context)

        result = await self.react(task, ctx, user_message)
        if isinstance(result, dict):
            # On failure, still record the attempt
            await self._record_run(ctx, today, 0, success=False)
            return result

        # --- Anti-hallucination gate ---
        # Read the on-disk report (canonical proof of what was synthesized)
        # to extract the URLs the LLM claims to have consulted. If none
        # appear, the LLM almost certainly hallucinated the report — refuse
        # to mark the run as successful so the operator gets a real signal.
        report_path = self._extract_report_path(result, today)
        report_text, source_label = await self._read_fresh_report(
            ctx, report_path, run_started_at
        )
        # Fall back to the LLM's final_answer text if the file isn't usable.
        if report_text is None:
            report_text = result
            source_label = "final_answer (no fresh report on disk)"
        articles = _extract_articles_from_report(report_text)

        if not articles:
            await self._record_run(ctx, today, 0, success=False)
            # Log the LLM's actual output and a sample of what was parsed —
            # without this the operator has no way to tell *why* the gate
            # tripped (LLM bailed? gave a recap? wrote prose without URLs?).
            preview_llm = result[:500].replace("\n", " ⏎ ")
            preview_parsed = (report_text or "")[:300].replace("\n", " ⏎ ")
            ctx.log(
                "warn",
                "veille-ia: no source URL found — run rejected. "
                f"source={source_label} ; "
                f"final_answer_preview={preview_llm!r} ; "
                f"parsed_preview={preview_parsed!r}",
            )
            return AIPResult.failed(
                "NO_SOURCES",
                "Le rapport ne cite aucune URL source. Le director a "
                "probablement halluciné la veille au lieu d'utiliser "
                "web_search / a2a:search-and-extract. Vérifie que les "
                "outils web sont activés (Réglages → Outils) et que les "
                "workers sont démarrés.",
            )

        # --- Persist seen articles for cross-day dedup ---
        new_count = await _persist_seen_articles(ctx, articles, today)
        article_count = len(articles)

        # --- Update memory after successful run ---
        await self._update_memory_post_run(ctx, today, result)
        await self._record_run(ctx, today, article_count, success=True, new_count=new_count)

        # --- v1.1.0: dual write + notify ---
        await _dual_write_report(ctx, today, report_path, result)
        await _notify_completion(ctx, today, article_count)

        return AIPResult.completed(result)

    def _build_user_message(
        self,
        today: str,
        snapshot: dict[str, Any],
        seen_hashes: list[str],
        user_context: dict[str, str] | None = None,
    ) -> str:
        tech_queries = snapshot.get("tech_queries", _INITIAL_SNAPSHOT["tech_queries"])
        competitive_queries = snapshot.get("competitive_queries", _INITIAL_SNAPSHOT["competitive_queries"])
        axes = snapshot.get("axes", _INITIAL_SNAPSHOT["axes"])
        report_dir = os.path.expanduser("~/.apollia/reports")
        report_path = f"{report_dir}/veille-{today}.md"

        # v1.1.0: bloc profil utilisateur si disponible
        user_block = ""
        user_context = user_context or {}
        if user_context:
            lines = ["**Profil utilisateur (lu depuis __user__ namespace) :**"]
            tools_value = user_context.get("user.tools.daily") or user_context.get(
                "user.tech.stack"
            )
            if tools_value:
                lines.append(
                    f"- Outils & stack du quotidien : {tools_value} "
                    "(adapter les axes tech : pondérer les techos liées à ces outils)"
                )
            languages_value = user_context.get("user.tech.languages")
            if languages_value:
                lines.append(f"- Langages familiers : {languages_value}")
            if "user.tech.proficiency" in user_context:
                lines.append(
                    f"- Aisance technique : {user_context['user.tech.proficiency']} "
                    "(doser le niveau de détail des explications)"
                )
            if "user.domain.sector" in user_context:
                lines.append(
                    f"- Secteur : {user_context['user.domain.sector']} "
                    "(les workers peuvent prioriser les news pertinentes pour ce secteur)"
                )
            user_block = "\n".join(lines) + "\n\n"

        return (
            f"Lance la veille quotidienne du {today}.\n\n"
            + user_block
            + f"**Requêtes tech ({len(tech_queries)}) :**\n"
            + json.dumps(tech_queries, ensure_ascii=False)
            + f"\n\n**Requêtes concurrentielles ({len(competitive_queries)}) :**\n"
            + json.dumps(competitive_queries, ensure_ascii=False)
            + f"\n\n**URLs déjà vues lors des runs précédents ({len(seen_hashes)}) "
            "(hashes 12 chars) :** \n"
            f"Si un article que tu trouves a un hash dans cette liste, c'est "
            f"un doublon — saute-le et cherche du contenu nouveau. Voici les "
            f"hashes à éviter (limité aux 50 plus récents) : "
            + json.dumps(seen_hashes[:50])
            + f"\n\n**Chemin du rapport à sauvegarder :** `{report_path}`\n\n"
            "**Étapes obligatoires** (chaque article du rapport doit citer "
            "son URL réelle entre crochets markdown `[titre](url)`) :\n"
            "1. Effectue la recherche (A2A si workers dispo, sinon `web_search` "
            "direct — minimum 2 requêtes par axe).\n"
            "2. Lis les meilleurs résultats avec `web_read`.\n"
            "3. Synthétise le rapport — chaque entrée porte son lien markdown.\n"
            "4. `file_write` du rapport.\n"
            "5. `final_answer` avec résumé.\n\n"
            "Si aucune URL réelle n'est citée, le run sera rejeté."
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
        new_count: int = 0,
    ) -> None:
        """Write an episodic memory record for this run."""
        if ctx.memory is None:
            return
        status = "succès" if success else "échec"
        if success:
            note = (
                f"Run du {today} : {article_count} articles "
                f"({new_count} nouveaux), {status}"
            )
        else:
            note = f"Run du {today} : {status} (aucun article retenu)"
        await ctx.memory.record(note, importance=0.7)

    async def _check_research_tools(self, ctx: Any) -> None:
        """Verify that web_search / web_read are actually registered.

        If `ctx.tools.describe(name)` returns None for these tools, the LLM
        will see no schema and bail to file_write hallucinations. Logging
        the issue here gives the operator a precise diagnostic instead of
        a generic NO_SOURCES rejection an hour later.
        """
        if ctx.tools is None:
            return
        for name in ("web_search", "web_read"):
            try:
                desc = await ctx.tools.describe(name)
            except Exception as e:
                ctx.log(
                    "warn",
                    f"veille-ia: ctx.tools.describe({name!r}) raised {e!r} — "
                    "research tool unavailable in registry.",
                )
                continue
            if desc is None:
                ctx.log(
                    "warn",
                    f"veille-ia: '{name}' is declared in tools_optional but "
                    "the runtime registry has no descriptor for it. The LLM "
                    "will likely fall back to hallucinating. Check that "
                    "Réglages → Outils has it enabled and that the runtime "
                    "binary embeds the 'web-search' / 'web-read' features.",
                )

    async def _read_fresh_report(
        self,
        ctx: Any,
        path: str,
        run_started_at: datetime,
    ) -> tuple[str | None, str]:
        """Re-read the report only if it was written during this run.

        Returns ``(content, source_label)``. If the file pre-dates the
        current run (left over from a previous successful run, including a
        hallucinated one), we treat it as stale and return ``(None, …)`` so
        the gate evaluates the LLM's actual final_answer text instead.

        Without this check the gate would either accept stale output as
        valid (false success) or reject blindly without telling the operator
        what was actually compared.
        """
        if ctx.tools is None:
            return None, "no ctx.tools"
        # Local mtime check — works even when file_read is sandboxed
        # because ~ expands to the user's home regardless.
        try:
            expanded = os.path.expanduser(path)
            mtime = datetime.fromtimestamp(os.path.getmtime(expanded))
            if mtime < run_started_at:
                # Stale — written by an earlier (probably hallucinated) run.
                return None, f"stale file mtime={mtime.isoformat()}"
        except FileNotFoundError:
            return None, "file not written"
        except Exception as e:
            # On unexpected errors, fall back to file_read and let it decide.
            ctx.log("debug", f"veille-ia: mtime check failed for {path}: {e}")

        try:
            content = await ctx.tools.call("file_read", {"path": path})
            return str(content), f"fresh report at {path}"
        except Exception as e:
            return None, f"file_read failed: {e}"

    @staticmethod
    def _extract_report_path(text: str, today: str) -> str:
        """Extract report path from final answer or return default."""
        match = re.search(r"(~?/[^\s]+veille[^\s]+\.md)", text)
        if match:
            return match.group(1)
        return f"~/.apollia/reports/veille-{today}.md"


agent = VeilleIaAgent()

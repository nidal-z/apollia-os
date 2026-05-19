"""veille-ia v3.0.0 — Director state machine appliquant les 4 piliers.

Refonte complète (M5a release v0.1.0) :
- Pilier 1 — Templates : Pydantic schemas (`schemas.py`) + Jinja2 (`templates/`).
- Pilier 2 — Steps fonctionnels : state machine 15 étapes (`state.py`).
- Pilier 3 — Datasources : YAML externalisés (`datasources/`), priorité ctx.workspace > local > defaults.
- Pilier 4 — Mémoire : entités (`entity:{type}:{id}`) + dédup (`seen:{hash}`) + procédurale.

Référence : docs/internal/release/M5-veille-ia-refonte-plan.md
État de l'art : docs/internal/research/agents-2026/
Skill ayant guidé la conception : .claude/skills/apollia-agent-forge/
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any

import jinja2
import yaml
from pydantic import ValidationError

from apollia import DomainError, agent, skill
from apollia.types import Ctx


def _capture_agent_dir() -> Path:
    """Snap le dossier de l'agent **à l'import du module**.

    `PyModule::from_code` (loader.rs:105) met `__file__` au simple nom de
    fichier (relatif), donc `Path(__file__).parent` est inutilisable.
    Le loader Rust insère le chemin absolu du dossier parent dans
    `sys.path[0]` *juste avant* d'exécuter ce fichier (loader.rs:90), donc
    cette valeur est fiable **uniquement à l'import-time**. Au runtime,
    un autre agent chargé entre-temps peut avoir poussé son propre path
    en tête de `sys.path`, ce qui ferait diverger les résolutions
    relatives (templates, datasources, etc.) vers le mauvais agent.
    """
    if sys.path:
        candidate = Path(sys.path[0])
        if candidate.is_absolute() and candidate.exists():
            return candidate
    # Fallback développement local (tests directs).
    return Path(__file__).resolve().parent


# Mémoïsation au top-level du module : capturée pendant `PyModule::from_code`,
# elle reste correcte pour toute la durée de vie du module — même si un autre
# agent vient ensuite éclipser `sys.path[0]`.
_AGENT_DIR: Path = _capture_agent_dir()


def _agent_dir() -> Path:
    """Retourne le dossier de l'agent (snap pris à l'import du module)."""
    return _AGENT_DIR

# Imports absolus (pas relatifs) car le runtime Apollia (PyModule::from_code via PyO3)
# charge le fichier sans contexte de package.
#
# IMPORTANT — Force purge sys.modules avant import : le runtime PyO3 recharge le
# director à chaque task via PyModule::from_code, MAIS les sub-imports
# (`from schemas import …`) sont résolus via `sys.modules` qui persiste tant que
# le daemon Apollia tourne. Sans purge, après un `apollia agent install` qui
# copie une nouvelle version de schemas.py, le director continue d'utiliser
# l'ancienne version cachée — d'où ValidationErrors persistantes même après fix.
for _local_mod_name in ("schemas", "state"):
    if _local_mod_name in sys.modules:
        del sys.modules[_local_mod_name]

from schemas import Article, Entity, EntitySignal, VeilleReport  # type: ignore[import-not-found]  # noqa: E402
from state import VeilleStep  # type: ignore[import-not-found]  # noqa: E402


CRITICAL_KEYWORDS = [
    "concurrent direct",
    "compétiteur direct",
    "Series A",
    "Series B",
    "Series C",
    "Series D",
    "fonds levés",
    "raised",
    "acquisition",
    "acquired",
    "breach",
    "0-day",
    "vulnérabilité critique",
    "MCP breaking",
    "open source",
]


@agent(
    name="veille-ia-agent",
    version="3.0.0",
    description="Veille IA/LLM quotidienne — state machine + entités + datasources YAML.",
    tags=("veille", "monitoring", "competitive-intelligence", "ia"),
    memory_namespace="veille-ia",
    tools_required=("file_write", "web_search", "web_read"),
    packages=("jinja2", "pydantic", "pyyaml"),
    step_budget={"max_steps": 25, "max_tool_calls": 25, "wall_clock_secs": 1800},
)
class VeilleIaAgent:
    """Director de la veille IA quotidienne — state machine déterministe."""

    SYSTEM_PROMPT = """Tu es veille-ia-agent, le director de la veille IA/LLM quotidienne.

<role>
Orchestre la production d'un rapport quotidien deux axes (technologique + concurrentiel) en déléguant aux workers spécialisés via A2A. Chaque step est déterministe ; le LLM n'est appelé que sur extract_entities, score_and_rank, et generate_exec_summary.
</role>

<output_format>
Rapport Markdown rendu via Jinja2 depuis un VeilleReport JSON validé Pydantic.
</output_format>
"""

    @skill(
        "veille.run",
        description="Lance la veille du jour et produit un rapport Markdown structuré.",
    )
    async def run_veille(
        self,
        prompt: str = "",
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Lance la state machine veille → produit le rapport Markdown du jour.

        Args:
            prompt: optional free-form instruction (e.g. "veille concurrentielle uniquement").
        """
        if ctx.llm is None:
            raise DomainError("NO_LLM", "Backend LLM requis")

        start_ts = datetime.now()
        state: dict[str, Any] = {
            "step": VeilleStep.INIT,
            "task": {"input": {"parts": [{"type": "text", "text": prompt}]}},
            "data": {},
            "progress": [],
            "errors": [],
            "today": start_ts.strftime("%Y-%m-%d"),
            "metrics": {
                "tool_calls": 0,
                "llm_calls": 0,
                "wall_clock_start": start_ts.timestamp(),
            },
        }

        max_iterations = 30
        iteration = 0
        while state["step"] != VeilleStep.DONE:
            if iteration >= max_iterations:
                raise DomainError(
                    "STATE_LOOP",
                    f"Max iterations atteint. Progress: {state['progress']}.",
                    details={"errors": state["errors"]},
                )
            iteration += 1
            try:
                state = await self._dispatch(state, ctx)
            except Exception as e:
                ctx.logger.error("step failed", step=state["step"].value, error=str(e))
                state["errors"].append({"step": state["step"].value, "error": str(e)})
                state["step"] = self._error_recovery(state["step"])

        state["metrics"]["wall_clock_secs"] = round(
            datetime.now().timestamp() - state["metrics"]["wall_clock_start"], 1
        )

        return {
            "report": state["data"].get("rendered_report", ""),
            "today": state["today"],
            "progress": state["progress"],
            "errors": state["errors"],
            "metrics": state["metrics"],
        }

    # ============================================================
    # Dispatch
    # ============================================================

    async def _dispatch(self, state: dict, ctx: Any) -> dict:
        handlers = {
            VeilleStep.INIT: self._step_init,
            VeilleStep.LOAD_DATASOURCES: self._step_load_datasources,
            VeilleStep.LOAD_USER_CONTEXT: self._step_load_user_context,
            VeilleStep.BOOTSTRAP_CHECK: self._step_bootstrap_check,
            VeilleStep.LOAD_ENTITIES: self._step_load_entities,
            VeilleStep.SEARCH_TECH: self._step_search_tech,
            VeilleStep.SEARCH_COMPETITIVE: self._step_search_competitive,
            VeilleStep.EXTRACT_ENTITIES: self._step_extract_entities,
            VeilleStep.SCORE_AND_RANK: self._step_score_and_rank,
            VeilleStep.DETECT_CRITICAL: self._step_detect_critical,
            VeilleStep.GENERATE_REPORT: self._step_generate_report,
            VeilleStep.PERSIST_MEMORY: self._step_persist_memory,
            VeilleStep.WRITE_FILE: self._step_write_file,
            VeilleStep.NOTIFY: self._step_notify,
        }
        handler = handlers.get(state["step"])
        if handler is None:
            raise RuntimeError(f"Pas de handler pour step {state['step'].value}")
        return await handler(state, ctx)

    def _error_recovery(self, current_step: VeilleStep) -> VeilleStep:
        """Politique d'erreur par step. Skippable = continuer ; non-skippable = abandonner."""
        transitions = {
            VeilleStep.LOAD_USER_CONTEXT: VeilleStep.BOOTSTRAP_CHECK,
            VeilleStep.LOAD_ENTITIES: VeilleStep.SEARCH_TECH,
            VeilleStep.SEARCH_TECH: VeilleStep.SEARCH_COMPETITIVE,
            VeilleStep.SEARCH_COMPETITIVE: VeilleStep.EXTRACT_ENTITIES,
            VeilleStep.EXTRACT_ENTITIES: VeilleStep.SCORE_AND_RANK,
            VeilleStep.DETECT_CRITICAL: VeilleStep.GENERATE_REPORT,
            VeilleStep.PERSIST_MEMORY: VeilleStep.WRITE_FILE,
            VeilleStep.WRITE_FILE: VeilleStep.NOTIFY,
            VeilleStep.NOTIFY: VeilleStep.DONE,
        }
        return transitions.get(current_step, VeilleStep.DONE)

    # ============================================================
    # Steps
    # ============================================================

    async def _step_init(self, state, ctx):
        ctx.logger.info(f"veille-ia-agent run started: {state['today']}")
        state["progress"].append("INIT ok")
        state["step"] = VeilleStep.LOAD_DATASOURCES
        return state

    async def _step_load_datasources(self, state, ctx):
        feeds = await self._load_yaml(ctx, "Veille IA — Feeds", "datasources/feeds.yaml")
        competitors = await self._load_yaml(ctx, "Veille IA — Competitors", "datasources/competitors.yaml")
        queries = await self._load_yaml(ctx, "Veille IA — Queries", "datasources/queries.yaml")
        scoring = await self._load_yaml(ctx, "Veille IA — Scoring", "datasources/scoring.yaml")

        state["data"]["feeds"] = feeds
        state["data"]["competitors"] = competitors
        state["data"]["queries"] = queries
        state["data"]["scoring"] = scoring

        state["progress"].append(
            f"DATASOURCES ok ({len(feeds.get('feeds', []))} feeds, "
            f"{len(competitors.get('competitors', []))} competitors)"
        )
        state["step"] = VeilleStep.LOAD_USER_CONTEXT
        return state

    async def _step_load_user_context(self, state, ctx):
        # ADR-087 — read the user profile through ctx.profile.  Flat keys
        # (`role`, `tech.stack`, …) live alongside the canonical schema.
        if ctx.profile is None:
            state["data"]["user_context"] = {}
            state["progress"].append("USER_CONTEXT skip (no profile)")
            state["step"] = VeilleStep.BOOTSTRAP_CHECK
            return state

        wanted = ["role", "tech.stack", "domain.sector", "agents.hitl"]
        ctx_user = {}
        try:
            all_entries = await ctx.profile.all()
            for key in wanted:
                if all_entries.get(key):
                    ctx_user[f"user.{key}"] = all_entries[key]
        except Exception as e:
            ctx.logger.debug(f"profile.all() failed: {e}")
        state["data"]["user_context"] = ctx_user
        state["progress"].append(f"USER_CONTEXT ok ({len(ctx_user)} keys)")
        state["step"] = VeilleStep.BOOTSTRAP_CHECK
        return state

    async def _step_bootstrap_check(self, state, ctx):
        if ctx.memory is None:
            state["progress"].append("BOOTSTRAP skip (no memory)")
            state["step"] = VeilleStep.LOAD_ENTITIES
            return state

        if await self._needs_bootstrap(ctx):
            snapshot = {
                "created_at": state["today"],
                "competitors": state["data"]["competitors"].get("competitors", []),
                "feeds": state["data"]["feeds"].get("feeds", []),
                "queries": state["data"]["queries"].get("axes", {}),
                "version": 1,
            }
            await ctx.memory.remember(
                "bootstrap.snapshot", json.dumps(snapshot, ensure_ascii=False), confidence=0.9
            )
            await ctx.memory.remember(
                "bootstrap.meta",
                json.dumps({"created_at": datetime.now().isoformat(), "version": 1}),
                confidence=1.0,
            )
            await ctx.memory.remember("bootstrap.status", "complete", confidence=1.0)
            state["progress"].append("BOOTSTRAP refreshed")
        else:
            state["progress"].append("BOOTSTRAP cached (TTL ok)")
        state["step"] = VeilleStep.LOAD_ENTITIES
        return state

    async def _step_load_entities(self, state, ctx):
        """Charge les entités connues + hashes d'URLs vues (dédup cross-run)."""
        if ctx.memory is None:
            state["data"]["seen_hashes"] = []
            state["data"]["known_entities"] = {}
            state["progress"].append("LOAD_ENTITIES skip (no memory)")
            state["step"] = VeilleStep.SEARCH_TECH
            return state

        try:
            seen_results = await ctx.memory.search("seen:", limit=500)
            seen_hashes = [
                r["key"].replace("seen:", "")
                for r in seen_results
                if r.get("key", "").startswith("seen:")
            ]
        except Exception as e:
            ctx.logger.warning(f"memory search seen: failed: {e}")
            seen_hashes = []

        try:
            entity_results = await ctx.memory.search("entity:", limit=200)
            known_entities = {}
            for r in entity_results:
                key = r.get("key", "")
                if key.startswith("entity:"):
                    try:
                        known_entities[key] = json.loads(r.get("value", "{}"))
                    except json.JSONDecodeError:
                        pass
        except Exception as e:
            ctx.logger.warning(f"memory search entity: failed: {e}")
            known_entities = {}

        state["data"]["seen_hashes"] = seen_hashes
        state["data"]["known_entities"] = known_entities
        state["progress"].append(
            f"LOAD_ENTITIES ok ({len(seen_hashes)} hashes, {len(known_entities)} entities)"
        )
        state["step"] = VeilleStep.SEARCH_TECH
        return state

    async def _step_search_tech(self, state, ctx):
        return await self._search_axis(state, ctx, "tech", VeilleStep.SEARCH_COMPETITIVE)

    async def _step_search_competitive(self, state, ctx):
        return await self._search_axis(state, ctx, "competitive", VeilleStep.EXTRACT_ENTITIES)

    async def _search_axis(self, state, ctx, axis: str, next_step: VeilleStep):
        queries_axis = state["data"]["queries"].get("axes", {}).get(axis, {})
        queries = queries_axis.get("queries", [])
        max_articles = state["data"]["queries"].get("depth", {}).get(axis, 5)

        if not queries:
            ctx.logger.warning(f"Aucune requête pour axe {axis}")
            state["data"][f"articles_{axis}_raw"] = []
            state["progress"].append(f"SEARCH_{axis.upper()} skip (no queries)")
            state["step"] = next_step
            return state

        articles: list[dict] = []
        skipped = 0
        a2a_used = False

        # Try A2A first (worker spécialisé si dispo)
        try:
            result = await ctx.a2a.invoke(
                "research.search_and_extract",
                {
                    "queries": queries,
                    "axis": axis,
                    "seen_hashes": state["data"]["seen_hashes"],
                    "max_articles": max_articles,
                },
                timeout_secs=300,
            )
            state["metrics"]["tool_calls"] += 1
            if result.get("status") == "completed":
                payload = result.get("result", {})
                articles_data = (
                    json.loads(payload.get("text", "{}"))
                    if isinstance(payload.get("text"), str)
                    else payload
                )
                articles = articles_data.get("articles", [])
                skipped = articles_data.get("skipped_dupes", 0)
                a2a_used = True
        except Exception as e:
            ctx.logger.info(f"a2a unavailable for {axis} ({e}) → fallback direct")

        # Fallback direct si A2A indispo OU 0 articles retournés
        if not a2a_used or not articles:
            articles, skipped = await self._direct_search_fallback(
                ctx, queries, axis, state["data"]["seen_hashes"], max_articles
            )

        state["data"][f"articles_{axis}_raw"] = articles
        state["metrics"][f"skipped_dupes_{axis}"] = skipped
        mode = "a2a" if a2a_used else "direct"
        state["progress"].append(
            f"SEARCH_{axis.upper()} ok ({len(articles)} articles, {mode})"
        )
        state["step"] = next_step
        return state

    async def _direct_search_fallback(
        self, ctx, queries: list[str], axis: str, seen_hashes: list[str], max_articles: int
    ) -> tuple[list[dict], int]:
        """Recherche web directe quand A2A n'est pas câblé au runtime.

        Le runtime Apollia (cf. `crates/apollia-cli/src/commands/start.rs:244`)
        ne câble `a2a_invoker` qu'en mode chat. Pour les agents lancés via
        trigger ou `apollia agent run`, on doit faire la recherche en direct.
        """
        seen_set = set(seen_hashes)
        articles: list[dict] = []
        skipped = 0
        # Limiter le nombre de requêtes pour rester dans le step budget
        for q in queries[: min(len(queries), 5)]:
            if len(articles) >= max_articles:
                break
            try:
                search_result = await ctx.tools.call("web_search", {"query": q, "max_results": 5})
                state_results = search_result.get("results", []) if isinstance(search_result, dict) else []
            except Exception as e:
                ctx.logger.warning(f"web_search failed for '{q}': {e}")
                continue

            for r in state_results:
                if len(articles) >= max_articles:
                    break
                url = r.get("url", "") if isinstance(r, dict) else ""
                if not url:
                    continue
                url_hash = hashlib.sha256(url.encode()).hexdigest()[:12]
                if url_hash in seen_set:
                    skipped += 1
                    continue
                # Excerpt par défaut depuis le snippet du moteur
                excerpt = r.get("snippet", "")[:2000] if isinstance(r, dict) else ""
                # Tenter web_read pour contenu complet (best-effort)
                try:
                    read_result = await ctx.tools.call("web_read", {"url": url, "max_length": 2000})
                    if isinstance(read_result, dict):
                        full_text = read_result.get("text") or read_result.get("content", "")
                        if full_text:
                            excerpt = full_text[:2000]
                except Exception as e:
                    ctx.logger.debug(f"web_read failed for {url}: {e}")
                source = url.split("/")[2] if "://" in url else url[:50]
                articles.append({
                    "title": (r.get("title") if isinstance(r, dict) else "") or url[:100],
                    "url": url,
                    "source": source,
                    "excerpt": excerpt,
                    "axis": axis,
                })
        return articles, skipped

    async def _step_extract_entities(self, state, ctx):
        """Extrait les entités via A2A si dispo, sinon fallback direct LLM."""
        all_articles = state["data"].get("articles_tech_raw", []) + state["data"].get(
            "articles_competitive_raw", []
        )
        if not all_articles:
            state["data"]["new_entities"] = []
            state["progress"].append("EXTRACT_ENTITIES skip (no articles)")
            state["step"] = VeilleStep.SCORE_AND_RANK
            return state

        entities: list[dict] = []
        article_to_entities: dict[str, list[str]] = {}
        a2a_used = False

        # Try A2A first
        try:
            result = await ctx.a2a.invoke(
                "research.extract_entities",
                {
                    "articles": all_articles,
                    "known_entities": list(state["data"].get("known_entities", {}).keys()),
                    "today": state["today"],
                },
                timeout_secs=180,
            )
            state["metrics"]["tool_calls"] += 1
            if result.get("status") == "completed":
                payload = result.get("result", {})
                ent_data = (
                    json.loads(payload.get("text", "{}"))
                    if isinstance(payload.get("text"), str)
                    else payload
                )
                entities = ent_data.get("entities", [])
                article_to_entities = ent_data.get("article_to_entities", {})
                a2a_used = True
        except Exception as e:
            ctx.logger.info(f"a2a extract-entities unavailable ({e}) → fallback direct LLM")

        # Fallback direct LLM
        if not a2a_used:
            entities, article_to_entities = await self._direct_extract_entities_fallback(
                ctx, all_articles, state["today"]
            )
            state["metrics"]["llm_calls"] += 1

        # Persiste les entités en mémoire
        new_entity_ids = []
        if ctx.memory:
            for ent in entities:
                if not isinstance(ent, dict) or "id" not in ent or "type" not in ent:
                    continue
                ent_id = f"entity:{ent['type']}:{ent['id']}"
                existing_raw = await ctx.memory.recall(ent_id)
                existing = json.loads(existing_raw) if existing_raw else None
                merged = self._merge_entity(existing, ent, state["today"])
                await ctx.memory.remember(
                    ent_id,
                    json.dumps(merged, ensure_ascii=False),
                    confidence=0.9,
                )
                if existing is None:
                    new_entity_ids.append(ent_id)

        state["data"]["new_entities"] = new_entity_ids
        state["data"]["article_to_entities"] = article_to_entities
        mode = "a2a" if a2a_used else "direct"
        state["progress"].append(
            f"EXTRACT_ENTITIES ok ({len(entities)} entities, {len(new_entity_ids)} new, {mode})"
        )
        state["step"] = VeilleStep.SCORE_AND_RANK
        return state

    _ENTITY_EXTRACTION_PROMPT = """Tu extrais les entités d'articles de veille IA/LLM.

Pour chaque entité (companies, products, events, topics), retourne :
{
  "entities": [
    {
      "id": "kebab-case-unique",
      "type": "company" | "product" | "event" | "topic",
      "name": "Nom officiel",
      "category": "direct" | "indirect" | "neutral",
      "threat_level": "low" | "medium" | "high",
      "summary": "1-2 phrases",
      "score": 1-5,
      "source_url": "url"
    }
  ],
  "article_to_entities": { "url1": ["entity:company:n8n"], ... }
}

Réponds UNIQUEMENT avec le JSON, max 15 entités, ne pas inventer ce qui n'est pas dans les articles."""

    async def _direct_extract_entities_fallback(
        self, ctx, articles: list[dict], today: str
    ) -> tuple[list[dict], dict[str, list[str]]]:
        """Fallback LLM direct quand A2A non câblé."""
        user_msg = json.dumps({"today": today, "articles": articles}, ensure_ascii=False)
        try:
            resp = await ctx.llm.chat(self._ENTITY_EXTRACTION_PROMPT, user_msg)
            text = self._extract_llm_text(resp)
            if not text:
                ctx.logger.warning(f"entity extraction: LLM returned empty text (resp type={type(resp).__name__})")
                return [], {}
            ctx.logger.info(f"entity extraction: LLM returned {len(text)} chars; preview={text[:200]!r}")
            cleaned = self._extract_json_block(text)
            if not cleaned:
                ctx.logger.warning("entity extraction: no JSON block found in LLM response")
                return [], {}
            parsed = json.loads(cleaned)
            return parsed.get("entities", []), parsed.get("article_to_entities", {})
        except Exception as e:
            ctx.logger.warning(f"direct entity extraction failed: {e}")
            return [], {}

    async def _step_score_and_rank(self, state, ctx):
        """Synthèse via A2A si dispo, sinon fallback direct LLM."""
        all_articles_tech = state["data"].get("articles_tech_raw", [])
        all_articles_competitive = state["data"].get("articles_competitive_raw", [])

        if not all_articles_tech and not all_articles_competitive:
            state["data"]["report"] = VeilleReport(
                date=state["today"],
                executive_summary="Aucun article significatif aujourd'hui.",
                articles_tech=[],
                articles_competitive=[],
                critical_findings=[],
                new_entities=state["data"].get("new_entities", []),
                metrics={"total_articles": 0},
            )
            state["progress"].append("SCORE_AND_RANK skip (no articles)")
            state["step"] = VeilleStep.DETECT_CRITICAL
            return state

        report: VeilleReport | None = None
        a2a_used = False

        # Try A2A first
        try:
            result = await ctx.a2a.invoke(
                "research.synthesize_report",
                {
                    "articles_tech": all_articles_tech,
                    "articles_competitive": all_articles_competitive,
                    "scoring": state["data"]["scoring"],
                    "user_context": state["data"]["user_context"],
                    "article_to_entities": state["data"].get("article_to_entities", {}),
                    "date": state["today"],
                },
                timeout_secs=300,
            )
            state["metrics"]["tool_calls"] += 1
            if result.get("status") == "completed":
                payload = result.get("result", {})
                raw_text = (
                    payload.get("text", "{}")
                    if isinstance(payload.get("text"), str)
                    else json.dumps(payload)
                )
                report = await self._validate_with_repair(raw_text, VeilleReport, ctx)
                a2a_used = True
        except Exception as e:
            ctx.logger.info(f"a2a synthesize-report unavailable ({e}) → fallback direct LLM")

        # Fallback direct LLM
        if report is None:
            report = await self._direct_synthesis_fallback(
                ctx,
                all_articles_tech,
                all_articles_competitive,
                state["data"]["scoring"],
                state["data"]["user_context"],
                state["data"].get("article_to_entities", {}),
                state["today"],
            )
            state["metrics"]["llm_calls"] += 1

        state["data"]["report"] = report
        mode = "a2a" if a2a_used else "direct"
        state["progress"].append(
            f"SCORE_AND_RANK ok (tech={len(report.articles_tech)}, "
            f"competitive={len(report.articles_competitive)}, {mode})"
        )
        state["step"] = VeilleStep.DETECT_CRITICAL
        return state

    _SYNTHESIS_PROMPT = """Tu es synthesis-worker, expert en synthèse de veille IA/LLM.

Produis un VeilleReport JSON conforme :
{
  "date": "YYYY-MM-DD",
  "executive_summary": "1-2 lignes en français",
  "articles_tech": [Article, ...],
  "articles_competitive": [Article, ...],
  "critical_findings": [],
  "new_entities": [],
  "metrics": {}
}

Article = {title, url, source, excerpt (max 2000 chars), score (1-5), score_stars (★),
           axis ("tech"|"competitive"), entities (list str), impact_apollia, impact_for_user, is_critical, matched_criteria}

Scoring : pondération additive selon `scoring.criteria` matchés. Score plafonné à 5.
score_stars : 5→"★★★★★", 4→"★★★★☆", 3→"★★★☆☆", 2→"★★☆☆☆", 1→"★☆☆☆☆".
Filter score < scoring.include_threshold. Max 6 articles par axe (top score).
Adapter executive_summary au user_context.user_role si fourni.

Réponds UNIQUEMENT avec le JSON, sans Markdown, sans texte avant/après."""

    async def _direct_synthesis_fallback(
        self,
        ctx,
        articles_tech: list[dict],
        articles_competitive: list[dict],
        scoring: dict,
        user_context: dict,
        article_to_entities: dict,
        today: str,
    ) -> VeilleReport:
        """Fallback synthèse direct LLM quand A2A non câblé."""
        user_msg = json.dumps(
            {
                "date": today,
                "articles_tech": articles_tech,
                "articles_competitive": articles_competitive,
                "scoring": scoring,
                "user_context": user_context,
                "article_to_entities": article_to_entities,
            },
            ensure_ascii=False,
        )
        try:
            resp = await ctx.llm.chat(self._SYNTHESIS_PROMPT, user_msg)
            text = self._extract_llm_text(resp)
            if not text:
                ctx.logger.warning(f"synthesis: LLM returned empty text (resp type={type(resp).__name__})")
                return self._build_minimal_report(today, articles_tech, articles_competitive, user_context)
            ctx.logger.info(f"synthesis: LLM returned {len(text)} chars; preview={text[:200]!r}")
            cleaned = self._extract_json_block(text)
            if not cleaned:
                ctx.logger.warning(f"synthesis: no JSON block found in LLM response; falling back to minimal report")
                return self._build_minimal_report(today, articles_tech, articles_competitive, user_context)
            return await self._validate_with_repair(cleaned, VeilleReport, ctx)
        except Exception as e:
            ctx.logger.warning(f"direct synthesis failed: {e}")
            # Avant d'abandonner, produire un rapport minimal des articles bruts
            return self._build_minimal_report(today, articles_tech, articles_competitive, user_context)

    @staticmethod
    def _extract_json_block(text: str) -> str:
        """Extrait le bloc JSON principal d'une réponse LLM.

        Gère :
        - Code fences ```json ... ```.
        - Prose narrative autour du JSON.
        - **Modèles "thinking"** (Qwen/DeepSeek-R1 et dérivés) qui émettent
          `<think>...</think>` avant le JSON. On strip ces blocs en premier.
        """
        if not text:
            return ""
        s = text.strip()
        # Strip blocs <think>...</think> (Qwen, DeepSeek-R1, etc.) — peut être multi-ligne, parfois non fermé
        s = re.sub(r"<think>.*?</think>", "", s, flags=re.DOTALL | re.IGNORECASE).strip()
        # Si <think> non fermé, couper tout ce qui est avant le premier {
        if "<think>" in s.lower():
            brace_idx = s.find("{")
            if brace_idx >= 0:
                s = s[brace_idx:]
        # Strip code fences
        s = re.sub(r"^```(?:json)?\s*|\s*```$", "", s, flags=re.MULTILINE).strip()
        # Chercher le premier { jusqu'au } correspondant (matching brace count)
        start = s.find("{")
        if start == -1:
            return ""
        depth = 0
        end = -1
        in_string = False
        escape = False
        for i in range(start, len(s)):
            c = s[i]
            if escape:
                escape = False
                continue
            if c == "\\" and in_string:
                escape = True
                continue
            if c == '"':
                in_string = not in_string
                continue
            if in_string:
                continue
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    end = i
                    break
        if end == -1:
            return ""
        return s[start : end + 1]

    def _build_minimal_report(
        self,
        today: str,
        articles_tech: list[dict],
        articles_competitive: list[dict],
        user_context: dict,
    ) -> VeilleReport:
        """Rapport minimal fabriqué côté Python quand le LLM ne produit pas de JSON exploitable.

        On préserve les articles trouvés (ce serait dommage de les perdre après 2m de scraping)
        en leur attribuant un score neutre 3 et en générant un excerpt depuis les données brutes.
        """
        def _to_article(raw: dict, axis: str) -> Article:
            return Article(
                title=raw.get("title", "")[:300] or "(Sans titre)",
                url=raw.get("url", ""),
                source=raw.get("source", "unknown"),
                excerpt=raw.get("excerpt", "")[:2000],
                score=3,
                score_stars="★★★☆☆",
                axis=axis,  # type: ignore[arg-type]
                entities=[],
                impact_apollia="(Synthèse LLM indisponible — articles bruts conservés)",
                impact_for_user="",
                is_critical=False,
                matched_criteria=[],
            )

        tech_arts = [_to_article(a, "tech") for a in articles_tech if a.get("url")]
        comp_arts = [_to_article(a, "competitive") for a in articles_competitive if a.get("url")]
        total = len(tech_arts) + len(comp_arts)
        role = user_context.get("user.role", "lecteur")
        return VeilleReport(
            date=today,
            executive_summary=(
                f"Synthèse LLM indisponible — {total} articles bruts conservés "
                f"(scoring neutre, sans analyse). Adapté pour {role}."
            ),
            articles_tech=tech_arts,
            articles_competitive=comp_arts,
            critical_findings=[],
            new_entities=[],
            metrics={"total_articles": total, "degraded": True},
        )

    async def _step_detect_critical(self, state, ctx):
        """Filtre les findings critiques (score >= threshold OR keyword match)."""
        report: VeilleReport = state["data"]["report"]
        threshold = state["data"]["scoring"].get("critical_threshold", 5)

        all_articles = report.articles_tech + report.articles_competitive
        critical = []
        for a in all_articles:
            is_crit = a.is_critical or a.score >= threshold
            if not is_crit:
                lower = (a.title + " " + a.excerpt).lower()
                if any(kw.lower() in lower for kw in CRITICAL_KEYWORDS):
                    is_crit = True
            if is_crit:
                a.is_critical = True
                critical.append(a)

        report.critical_findings = critical
        state["data"]["report"] = report
        state["progress"].append(f"DETECT_CRITICAL ok ({len(critical)} critical)")
        state["step"] = VeilleStep.GENERATE_REPORT
        return state

    async def _step_generate_report(self, state, ctx):
        """Rendu Jinja2 du VeilleReport."""
        report: VeilleReport = state["data"]["report"]
        all_articles = report.articles_tech + report.articles_competitive
        report.metrics = {
            "total_articles": len(all_articles),
            "avg_score": (sum(a.score for a in all_articles) / len(all_articles)) if all_articles else 0.0,
            "skipped_dupes": (
                state["metrics"].get("skipped_dupes_tech", 0)
                + state["metrics"].get("skipped_dupes_competitive", 0)
            ),
            "tool_calls": state["metrics"]["tool_calls"],
            "wall_clock_secs": round(
                datetime.now().timestamp() - state["metrics"]["wall_clock_start"], 1
            ),
        }
        state["data"]["report"] = report

        rendered = self._render_template(
            "report.md.j2",
            date=report.date,
            executive_summary=report.executive_summary,
            articles_tech=[a.model_dump() for a in report.articles_tech],
            articles_competitive=[a.model_dump() for a in report.articles_competitive],
            critical_findings=[a.model_dump() for a in report.critical_findings],
            new_entities=report.new_entities,
            metrics=report.metrics,
            user_context=state["data"].get("user_context", {}),
        )
        state["data"]["rendered_report"] = rendered
        state["progress"].append(f"GENERATE_REPORT ok ({len(rendered)} chars)")
        state["step"] = VeilleStep.PERSIST_MEMORY
        return state

    async def _step_persist_memory(self, state, ctx):
        """Persiste épisode + seen:{hash} pour les nouvelles URLs."""
        if ctx.memory is None:
            state["progress"].append("PERSIST skip (no memory)")
            state["step"] = VeilleStep.WRITE_FILE
            return state

        report: VeilleReport = state["data"]["report"]
        await ctx.memory.record(
            content=(
                f"Run {state['today']} : {report.metrics.get('total_articles', 0)} articles, "
                f"{len(report.critical_findings)} critiques"
            ),
            importance=0.7,
            task_id=state["task"].get("task_id"),
            metadata={"date": state["today"], "metrics": report.metrics},
        )
        await ctx.memory.remember("last_run_date", state["today"], source=state["task"].get("task_id"))

        all_articles = report.articles_tech + report.articles_competitive + report.critical_findings
        seen_count = 0
        for a in all_articles:
            url_hash = hashlib.sha256(a.url.encode()).hexdigest()[:12]
            seen_key = f"seen:{url_hash}"
            already = await ctx.memory.recall(seen_key)
            if not already:
                await ctx.memory.remember(
                    seen_key,
                    json.dumps(
                        {
                            "title": a.title[:200],
                            "url": a.url,
                            "source": a.source,
                            "date": state["today"],
                        },
                        ensure_ascii=False,
                    ),
                    confidence=1.0,
                )
                seen_count += 1

        try:
            existing_proc = await ctx.memory.recall_procedure("daily-veille")
            if not existing_proc:
                await ctx.memory.learn_procedure(
                    "daily-veille",
                    [
                        "Charger datasources YAML (feeds, competitors, queries, scoring)",
                        "Recharger user.* + entités connues",
                        "Bootstrap snapshot si TTL > 7j",
                        "Délégation A2A search-and-extract (axes tech + competitive) avec dédup",
                        "Délégation A2A extract-entities pour upsert entity:* mémoire",
                        "Délégation A2A synthesize-report pour scoring + Pydantic VeilleReport",
                        "Filter critical findings (threshold + keywords)",
                        "Rendu Jinja2 + écriture fichier + notification",
                    ],
                )
        except Exception as e:
            ctx.logger.debug(f"learn_procedure failed: {e}")

        state["progress"].append(f"PERSIST ok ({seen_count} new seen, episode recorded)")
        state["step"] = VeilleStep.WRITE_FILE
        return state

    async def _step_write_file(self, state, ctx):
        """Sauvegarde du rapport sur disque."""
        rendered = state["data"]["rendered_report"]
        path = f"~/.apollia/reports/veille-{state['today']}.md"
        try:
            await ctx.tools.call("file_write", {"path": path, "content": rendered})
            state["metrics"]["tool_calls"] += 1
            state["progress"].append(f"WRITE_FILE ok ({path})")
        except Exception as e:
            ctx.logger.warning(f"file_write failed: {e}")
            state["progress"].append(f"WRITE_FILE failed: {e}")
        state["step"] = VeilleStep.NOTIFY
        return state

    async def _step_notify(self, state, ctx):
        """Notifications : desktop + alerte critique webhook si applicable."""
        report: VeilleReport = state["data"]["report"]
        articles_count = len(report.articles_tech) + len(report.articles_competitive)
        critical_count = len(report.critical_findings)

        if ctx.notify:
            top_3 = sorted(
                report.articles_tech + report.articles_competitive,
                key=lambda a: a.score,
                reverse=True,
            )[:3]
            summary = self._render_template(
                "summary.md.j2",
                agent_name="veille-ia",
                date=state["today"],
                articles_count=articles_count,
                critical_count=critical_count,
                top_3=[a.model_dump() for a in top_3],
            )
            severity = "warning" if critical_count > 0 else "info"
            try:
                await ctx.notify.publish(
                    message=summary,
                    severity=severity,
                    title="Apollia · Veille IA",
                )
            except Exception as e:
                ctx.logger.warning(f"notify desktop failed: {e}")

            if critical_count > 0:
                alert = self._render_template(
                    "alert.md.j2",
                    date=state["today"],
                    critical_findings=[a.model_dump() for a in report.critical_findings],
                )
                try:
                    await ctx.notify.publish(
                        message=alert,
                        severity="warning",
                        title="⚠️ Veille IA — Finding critique",
                        channel="webhook",
                    )
                except Exception as e:
                    ctx.logger.warning(f"notify webhook failed: {e}")

        state["progress"].append(f"NOTIFY ok ({articles_count} articles, {critical_count} critical)")
        state["step"] = VeilleStep.DONE
        return state

    # ============================================================
    # Helpers
    # ============================================================

    async def _load_yaml(self, ctx, workspace_title: str, local_relative: str) -> dict:
        """Lecture priorisée : ctx.workspace > local YAML > {} fallback."""
        if ctx.workspace:
            ws_yaml = ctx.workspace.get(workspace_title)
            if ws_yaml:
                try:
                    return yaml.safe_load(ws_yaml) or {}
                except yaml.YAMLError as e:
                    ctx.logger.warning(f"workspace YAML invalid for '{workspace_title}': {e}")
        local_path = _agent_dir() / local_relative
        if local_path.exists():
            try:
                return yaml.safe_load(local_path.read_text()) or {}
            except yaml.YAMLError as e:
                ctx.logger.warning(f"local YAML invalid {local_path}: {e}")
        return {}

    async def _needs_bootstrap(self, ctx) -> bool:
        if ctx.memory is None:
            return False
        meta_raw = await ctx.memory.recall("bootstrap.meta")
        if not meta_raw:
            return True
        try:
            meta = json.loads(meta_raw)
            last = datetime.fromisoformat(meta["created_at"])
            return (datetime.now() - last) > timedelta(days=7)
        except Exception:
            return True

    def _merge_entity(self, existing: dict | None, new: dict, today: str) -> dict:
        """Merge incrémental d'une entité avec timeline."""
        signal = {
            "date": today,
            "summary": new.get("summary", ""),
            "source_url": new.get("source_url", ""),
            "score": int(new.get("score", 3)),
        }
        if not existing:
            return {
                "id": new.get("id"),
                "type": new.get("type"),
                "name": new.get("name"),
                "category": new.get("category", "direct"),
                "threat_level": new.get("threat_level", "medium"),
                "first_seen": today,
                "last_event_date": today,
                "signals": [signal],
            }
        existing["last_event_date"] = today
        existing.setdefault("signals", []).append(signal)
        return existing

    async def _validate_with_repair(self, raw: str, schema, ctx, max_retries: int = 3):
        """Validation Pydantic avec auto-repair (re-prompt LLM en cas de ValidationError)."""
        for attempt in range(max_retries):
            try:
                return schema.model_validate_json(raw)
            except ValidationError as e:
                ctx.logger.warning(f"Validation failed (attempt {attempt + 1}/{max_retries}): {e}")
                if attempt == max_retries - 1:
                    raise
                repair_prompt = (
                    f"Le JSON suivant ne respecte pas le schema attendu.\n"
                    f"Erreur: {e}\n\n"
                    f"JSON original:\n{raw}\n\n"
                    f"Retourne un JSON corrigé conforme au schema. Réponds avec le JSON uniquement."
                )
                resp = await ctx.llm.complete([{"role": "user", "content": repair_prompt}])
                raw_text = self._extract_llm_text(resp)
                # Re-appliquer extraction JSON robuste : strip <think>, code fences, brace matching
                raw = self._extract_json_block(raw_text) or raw_text.strip()
        raise RuntimeError(f"Auto-repair failed after {max_retries}")

    def _fallback_report(self, state: dict) -> VeilleReport:
        """Rapport minimal si la synthesis échoue."""
        return VeilleReport(
            date=state["today"],
            executive_summary=f"Run dégradé : {len(state['errors'])} erreurs durant la synthèse.",
            articles_tech=[],
            articles_competitive=[],
            critical_findings=[],
            new_entities=state["data"].get("new_entities", []),
            metrics={"degraded": True, "errors_count": len(state["errors"])},
        )

    @staticmethod
    def _extract_llm_text(resp: Any) -> str:
        """Extrait le texte d'une LlmResponse PyO3.

        L'attribut exposé par le runtime est `content` (cf. crates/apollia-aip/src/llm.rs:54-65),
        pas `text`. On gère les deux noms par défense : selon les variantes du SDK
        (chat() vs complete() vs run_tools), l'API peut évoluer.
        """
        if resp is None:
            return ""
        for attr in ("content", "text", "message", "output"):
            value = getattr(resp, attr, None)
            if isinstance(value, str):
                return value
        # Dernier recours : str() — peut retourner une repr() inutilisable comme `<...object at 0x...>`,
        # mais évite de péter avec une exception en aval.
        s = str(resp)
        if s.startswith("<") and "object at 0x" in s:
            return ""  # signal explicite : pas de texte exploitable
        return s

    def _render_template(self, template_name: str, **vars) -> str:
        env = jinja2.Environment(
            loader=jinja2.FileSystemLoader(str(_agent_dir() / "templates")),
            autoescape=False,
            trim_blocks=True,
            lstrip_blocks=True,
        )
        return env.get_template(template_name).render(**vars)


# (Module-level ``agent`` singleton exposé automatiquement par le décorateur
# ``@agent`` (ADR-107).)

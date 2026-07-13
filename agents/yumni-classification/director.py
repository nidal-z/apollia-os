"""RSE classification director: deterministic pipelines over the workers.

Two modes, one director (chosen by the incoming message ``mode``):

  * ``entity``   : 1 action -> N criteria.  (propose -> verify -> threshold)
  * ``criterion``: 1 criterion -> N actions. (match  -> verify -> threshold)

IMPORTANT: the AI NEVER writes to Yumni's data. It only READS (action context,
criteria, actions) and produces PROPOSALS. The accepted proposals are handed back
to Yumni via ``complete_classification`` and stored on the classification *job*
for human review. A real mapping is created only when a human validates in the UI.
This keeps the AI strictly suggestion-only: nothing lands in the mappings table
without a human decision.

Business rules live here: kept only if the verifier says keep AND confidence >=
THRESHOLD; deduped; justifications stay descriptive (enforced in the workers).
Yumni I/O goes through the ``yumni`` MCP connector (read tools + complete).
Workers are reached via A2A.
"""

from __future__ import annotations

import asyncio
import json
from typing import Any

from apollia import DomainError, agent, on_message
from apollia.types import Ctx, Message
from apollia.utils import a2a_result_data

# Keep a candidate only if the verifier's confidence clears this bar.
THRESHOLD = 0.6
# Mode B is O(K) LLM calls, bound the number of candidate actions scanned.
MAX_ACTIONS = 12

# Yumni MCP connector tools (server "yumni" registered in mcp.toml).
# NOTE: no create_mapping, the director does not write mappings.
TOOL_GET_CONTEXT = "mcp:yumni/get_action_context"
TOOL_LIST_CRITERIA = "mcp:yumni/list_criteria"
TOOL_LIST_ACTIONS = "mcp:yumni/list_actions"
TOOL_COMPLETE = "mcp:yumni/complete_classification"


def _payload(message: str) -> dict[str, Any]:
    try:
        p = json.loads(message)
    except (json.JSONDecodeError, TypeError) as exc:
        raise DomainError(
            "BAD_MESSAGE",
            'message must be JSON, e.g. {"mode":"entity","actionId":"..."}',
            details={"raw": str(message)[:300]},
        ) from exc
    if not isinstance(p, dict):
        raise DomainError("BAD_MESSAGE", "message JSON must be an object")
    return p


@agent(
    name="rse-classification-director",
    version="0.3.0",
    description=(
        "Director that classifies Yumni data against referential criteria in two "
        "directions (action->criteria, criterion->actions) via a deterministic "
        "propose/match -> verify -> threshold pipeline. Read-only + proposals: it "
        "never writes mappings; a human validates in Yumni."
    ),
    tags=("rse", "esrs", "classification", "director"),
    tools_required=(
        TOOL_GET_CONTEXT,
        TOOL_LIST_CRITERIA,
        TOOL_LIST_ACTIONS,
        TOOL_COMPLETE,
    ),
    agent_type="assistant",
    # Parallélisme : jusqu'à N classifications simultanées. Au-delà, Yumni met en
    # file (retry). Les workers (mapper/verifier) doivent avoir un cap >= N, sinon
    # les tâches parallèles se percutent lors des appels A2A.
    max_concurrent_tasks=3,
)
class RseClassificationDirector:
    """Orchestrates the ESRS classification pipeline (both directions)."""

    THRESHOLD = THRESHOLD

    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        """Run a classification and return a structured JSON report.

        Message: ``{"mode":"entity","actionId":"...","jobId":"..."}`` or
        ``{"mode":"criterion","criterionCode":"E1","jobId":"..."}``.
        """
        payload = _payload(message)
        mode = str(payload.get("mode") or "entity").strip().lower()
        job_id = payload.get("jobId")

        try:
            if mode == "criterion":
                report = await self._classify_criterion(ctx, payload)
            else:
                report = await self._classify_entity(ctx, payload)
        except DomainError as exc:
            if job_id:
                await self._safe_complete(ctx, str(job_id), [], 0, error=f"{exc.code}: {exc.message}")
            raise

        if job_id:
            await self._safe_complete(
                ctx, str(job_id), report.get("accepted", []), int(report.get("proposedCount", 0))
            )
        return json.dumps({"result": report}, ensure_ascii=False)

    # ---- Direction A: 1 action -> N criteria -----------------------------
    async def _classify_entity(self, ctx: Ctx, payload: dict[str, Any]) -> dict[str, Any]:
        action_id = payload.get("actionId") or (
            payload.get("entityId") if payload.get("entityType") == "action" else None
        )
        if not action_id or not isinstance(action_id, str):
            raise DomainError("BAD_MESSAGE", 'mode "entity" requires actionId')
        ctx.logger.info("classify entity: start", action_id=action_id)

        context = _mcp_data(await ctx.tools.call(TOOL_GET_CONTEXT, {"actionId": action_id}))
        criteria = _coerce_criteria(_mcp_data(await ctx.tools.call(TOOL_LIST_CRITERIA, {})))
        if not criteria:
            raise DomainError("NO_CRITERIA", "list_criteria returned no criteria.")
        entity = _coerce_entity(context, action_id)
        allowed = {str(c.get("code")): c for c in criteria if c.get("code")}

        props = a2a_result_data(
            await ctx.a2a.invoke("esrs.propose_mappings", {"entity": entity, "criteria": criteria})
        ) or {}
        candidates = props.get("mappings", []) if isinstance(props, dict) else []

        # Verify every candidate CONCURRENTLY. Each verification is independent
        # (one candidate against one criterion), so on a batching backend the N
        # calls run in parallel instead of one-after-another. This is the main
        # latency win: it collapses the sequential verify chain into a single
        # round (bounded by the esrs-verifier worker's concurrency cap).
        work = [
            (cand, code, criterion)
            for cand in candidates
            for code in (str(cand.get("criterionCode", "")).strip(),)
            for criterion in (allowed.get(code),)
            if criterion is not None
        ]
        verdicts = await asyncio.gather(
            *(
                self._verify(ctx, entity, code, cand.get("justification", ""), criterion)
                for cand, code, criterion in work
            )
        )

        accepted_by_code: dict[str, dict[str, Any]] = {}
        rejected: list[dict[str, Any]] = []
        for (cand, code, criterion), (conf, keep, reason) in zip(work, verdicts):
            proposal = {
                "entityType": "action",
                "entityId": action_id,
                "entityTitle": entity.get("title", ""),
                "criterionCode": code,
                "criterionTitle": criterion.get("title", ""),
                "confidence": conf,
                "justification": cand.get("justification", ""),
            }
            if not keep:
                rejected.append({**proposal, "verifierReason": reason})
                continue
            cur = accepted_by_code.get(code)
            if cur is None or conf > cur["confidence"]:
                accepted_by_code[code] = proposal

        return {
            "mode": "entity",
            "actionId": action_id,
            "proposedCount": len(candidates),
            "accepted": list(accepted_by_code.values()),
            "rejected": rejected,
        }

    # ---- Direction B: 1 criterion -> N actions ---------------------------
    async def _classify_criterion(self, ctx: Ctx, payload: dict[str, Any]) -> dict[str, Any]:
        code = str(payload.get("criterionCode") or "").strip()
        if not code:
            raise DomainError("BAD_MESSAGE", 'mode "criterion" requires criterionCode')
        ctx.logger.info("classify criterion: start", criterion=code)

        ctx.logger.info("calling TOOL_LIST_CRITERIA")
        criteria = _coerce_criteria(_mcp_data(await ctx.tools.call(TOOL_LIST_CRITERIA, {})))
        ctx.logger.info("criteria loaded", count=len(criteria))
        criterion = next((c for c in criteria if str(c.get("code")) == code), None)
        if criterion is None:
            raise DomainError("UNKNOWN_CRITERION", f"criterion {code!r} not in referential")
        ctx.logger.info("criterion found", code=code)

        ctx.logger.info("calling TOOL_LIST_ACTIONS")
        actions = _coerce_actions(_mcp_data(await ctx.tools.call(TOOL_LIST_ACTIONS, {})))[:MAX_ACTIONS]
        ctx.logger.info("actions loaded", count=len(actions))
        if not actions:
            ctx.logger.warning("no actions found, returning empty")
            return {"mode": "criterion", "criterionCode": code, "proposedCount": 0,
                    "accepted": [], "rejected": []}
        by_id = {str(a["id"]): a for a in actions if a.get("id")}

        ctx.logger.info("invoking esrs.match_entities A2A", entities_count=len(actions))
        matched = a2a_result_data(
            await ctx.a2a.invoke("esrs.match_entities", {"criterion": criterion, "entities": actions})
        ) or {}
        ctx.logger.info("A2A match_entities returned", matched_type=type(matched).__name__)
        candidates = matched.get("matches", []) if isinstance(matched, dict) else []
        ctx.logger.info("candidates extracted", count=len(candidates))

        # Same concurrency win as direction A: verify all matched actions at once
        # (each is an independent action-vs-criterion check).
        work = [
            (cand, entity_id, action)
            for cand in candidates
            for entity_id in (str(cand.get("entityId", "")).strip(),)
            for action in (by_id.get(entity_id),)
            if action is not None
        ]
        verdicts = await asyncio.gather(
            *(
                self._verify(ctx, action, code, cand.get("justification", ""), criterion)
                for cand, entity_id, action in work
            )
        )

        accepted_by_id: dict[str, dict[str, Any]] = {}
        rejected: list[dict[str, Any]] = []
        for (cand, entity_id, action), (conf, keep, reason) in zip(work, verdicts):
            proposal = {
                "entityType": "action",
                "entityId": entity_id,
                "entityTitle": action.get("title", ""),
                "criterionCode": code,
                "criterionTitle": criterion.get("title", ""),
                "confidence": conf,
                "justification": cand.get("justification", ""),
            }
            if not keep:
                rejected.append({**proposal, "verifierReason": reason})
                continue
            cur = accepted_by_id.get(entity_id)
            if cur is None or conf > cur["confidence"]:
                accepted_by_id[entity_id] = proposal

        return {
            "mode": "criterion",
            "criterionCode": code,
            "proposedCount": len(candidates),
            "accepted": list(accepted_by_id.values()),
            "rejected": rejected,
        }

    # ---- shared verify ----------------------------------------------------
    async def _verify(
        self, ctx: Ctx, entity: dict, code: str, justification: str, criterion: dict
    ) -> tuple[float, bool, str]:
        verdict = a2a_result_data(
            await ctx.a2a.invoke(
                "esrs.verify_mapping",
                {
                    "entity": entity,
                    "candidate": {"criterionCode": code, "justification": justification},
                    "criterion": criterion,
                },
            )
        ) or {}
        conf = float(verdict.get("confidence", 0.0))
        keep = bool(verdict.get("keep")) and conf >= THRESHOLD
        return conf, keep, str(verdict.get("reason", ""))

    async def _safe_complete(
        self, ctx: Ctx, job_id: str, proposals: list[dict[str, Any]], proposed: int,
        error: str | None = None,
    ) -> None:
        """Hand proposals back to Yumni (stored on the job; best-effort)."""
        try:
            body: dict[str, Any] = {
                "jobId": job_id,
                "proposedCount": proposed,
                "writtenCount": 0,
                "proposals": proposals,
            }
            if error:
                body["error"] = error
            await ctx.tools.call(TOOL_COMPLETE, body)
        except Exception as exc:  # noqa: BLE001, completion is best-effort
            ctx.logger.error("complete_classification failed", job_id=job_id, error=str(exc))


def _mcp_data(raw: Any) -> Any:
    """Unwrap an MCP tool result: ``{"content": "<json>"}`` or content blocks."""
    if isinstance(raw, dict) and "content" in raw:
        c = raw["content"]
        if isinstance(c, list) and c and isinstance(c[0], dict) and "text" in c[0]:
            c = c[0]["text"]
        if isinstance(c, str):
            try:
                return json.loads(c)
            except (json.JSONDecodeError, TypeError):
                return c
    return raw


def _coerce_criteria(raw: Any) -> list[dict[str, Any]]:
    """Normalize list_criteria into ``[{code,title,description}]``."""
    if isinstance(raw, dict):
        for key in ("criteria", "items", "result", "data"):
            if isinstance(raw.get(key), list):
                raw = raw[key]
                break
    if not isinstance(raw, list):
        return []
    out: list[dict[str, Any]] = []
    for c in raw:
        if isinstance(c, dict) and c.get("code"):
            out.append({"code": str(c["code"]), "title": str(c.get("title", "")),
                        "description": str(c.get("description", ""))})
    return out


def _coerce_actions(raw: Any) -> list[dict[str, Any]]:
    """Normalize list_actions into ``[{id,title,description}]``."""
    if isinstance(raw, dict):
        for key in ("actions", "items", "result", "data"):
            if isinstance(raw.get(key), list):
                raw = raw[key]
                break
    if not isinstance(raw, list):
        return []
    out: list[dict[str, Any]] = []
    for a in raw:
        if isinstance(a, dict) and a.get("id"):
            out.append({"id": str(a["id"]), "title": str(a.get("title", "")),
                        "description": str(a.get("description", "") or "")})
    return out


def _coerce_entity(raw: Any, action_id: str) -> dict[str, Any]:
    """Normalize get_action_context into an entity dict."""
    src: dict[str, Any] = {}
    if isinstance(raw, dict):
        for key in ("action", "entity", "result", "data"):
            if isinstance(raw.get(key), dict):
                src = raw[key]
                break
        else:
            src = raw
    return {
        "id": action_id,
        "title": str(src.get("title", "")),
        "description": str(src.get("description", "")),
        "context": str(src.get("context", "")),
    }


agent = RseClassificationDirector()  # type: ignore[assignment]

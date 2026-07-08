"""Yumni bridge worker — Yumni REST I/O over A2A.

Why this exists: in this Apollia build, an agent's ``ctx.tools.call("mcp:yumni/*")``
resolves the tool but fails to EXECUTE it ("unknown tool") — the MCP execution
path from the AIP ToolProxy is not wired. A2A (``ctx.a2a.invoke``) works, so we
expose Yumni's read/write as A2A skills here. The director calls these instead of
MCP tools, which closes the loop end-to-end on the working path.

(The ``yumni-mcp`` connector remains the target integration surface once Apollia's
MCP execution path is fixed; this worker is the PoC equivalent.)

Least privilege is enforced SERVER-SIDE by Yumni: the scoped JWT can read the
source entities and create ``suggested`` mappings only — nothing else.
"""

from __future__ import annotations

import asyncio
import json
import os
import urllib.request
import urllib.error
from pathlib import Path
from typing import Any

from apollia import DomainError, agent, skill

_API = os.environ.get("YUMNI_API_URL", "http://localhost:8080/api").rstrip("/")
_REF = os.environ.get("YUMNI_REFERENTIAL_CODE", "esrs")
_TOKEN_FILE = Path.home() / ".apollia" / "yumni-ai-token.txt"


def _token() -> str:
    tok = os.environ.get("YUMNI_AI_TOKEN", "").strip()
    if not tok and _TOKEN_FILE.exists():
        tok = _TOKEN_FILE.read_text(encoding="utf-8").strip()
    if not tok:
        raise DomainError(
            "NO_TOKEN",
            "YUMNI_AI_TOKEN missing (env or ~/.apollia/yumni-ai-token.txt).",
        )
    return tok


def _request(method: str, path: str, body: dict | None = None) -> Any:
    url = f"{_API}{path}"
    data = json.dumps(body).encode("utf-8") if body is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Authorization", f"Bearer {_token()}")
    req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            raw = resp.read().decode("utf-8")
            return json.loads(raw) if raw else None
    except urllib.error.HTTPError as exc:  # 4xx/5xx from Yumni
        detail = exc.read().decode("utf-8", "replace")[:500]
        raise DomainError(
            "YUMNI_HTTP_ERROR",
            f"{method} {path} -> {exc.code}",
            details={"status": exc.code, "body": detail},
        ) from exc
    except urllib.error.URLError as exc:  # connection refused, etc.
        raise DomainError(
            "YUMNI_UNREACHABLE", f"{method} {path}: {exc.reason}"
        ) from exc


async def _aget(path: str) -> Any:
    return await asyncio.to_thread(_request, "GET", path, None)


async def _apost(path: str, body: dict) -> Any:
    return await asyncio.to_thread(_request, "POST", path, body)


def _flatten(nodes: list[dict], out: list[dict]) -> None:
    for n in nodes:
        out.append(n)
        kids = n.get("children")
        if isinstance(kids, list):
            _flatten(kids, out)


@agent(
    name="yumni-bridge",
    version="0.1.0",
    description="A2A bridge to Yumni's REST API (read source entities, write suggested mappings).",
    tags=("yumni", "bridge", "rest"),
    agent_type="worker",
)
class YumniBridge:
    """Thin, typed A2A proxy over Yumni's referential + entity endpoints."""

    @skill(
        "yumni.get_action_context",
        description="Fetch a Yumni action and its parent context (project/objective/axis).",
    )
    async def get_action_context(self, actionId: str) -> dict[str, Any]:
        action = await _aget(f"/actions/{actionId}")
        if not isinstance(action, dict):
            raise DomainError("NOT_FOUND", f"action {actionId} not found")
        ctx_parts: list[str] = []
        project = None
        if action.get("projectId"):
            try:
                project = await _aget(f"/projects/{action['projectId']}")
            except DomainError:
                project = None
        if isinstance(project, dict):
            ctx_parts.append(f"Projet: {project.get('name', '')}")
        return {
            "title": action.get("title", ""),
            "description": action.get("description", "") or "",
            "context": " | ".join([p for p in ctx_parts if p]),
        }

    @skill(
        "yumni.list_criteria",
        description="List the ESRS criteria (closed list) as [{code,title,description}].",
    )
    async def list_criteria(self) -> dict[str, Any]:
        tree = await _aget(f"/referentials/{_REF}/criteria")
        flat: list[dict] = []
        if isinstance(tree, list):
            _flatten(tree, flat)
        criteria = [
            {
                "code": str(c.get("code")),
                "title": str(c.get("title", "")),
                "description": str(c.get("description", "") or ""),
            }
            for c in flat
            if c.get("kind") == "datapoint" and c.get("code")
        ]
        return {"criteria": criteria}

    @skill(
        "yumni.create_mapping",
        description="Create a SUGGESTED (source=ai) mapping in Yumni; a human validates it later.",
    )
    async def create_mapping(
        self,
        entityType: str,
        entityId: str,
        criterionCode: str,
        confidence: float = 0.0,
        justification: str = "",
    ) -> dict[str, Any]:
        body = {
            "entityType": entityType,
            "entityId": entityId,
            "criterionCode": criterionCode,
            "source": "ai",
            "status": "suggested",
            "confidence": confidence,
            "justification": justification,
        }
        try:
            created = await _apost(f"/referentials/{_REF}/mappings", body)
        except DomainError as exc:
            # Idempotent: a duplicate suggestion (409) is not an error for a
            # re-runnable classification — treat it as already-suggested.
            if str((exc.details or {}).get("status")) == "409":
                return {"status": "already_suggested", "criterionCode": criterionCode}
            raise
        return created if isinstance(created, dict) else {"created": created}


agent = YumniBridge()  # type: ignore[assignment]

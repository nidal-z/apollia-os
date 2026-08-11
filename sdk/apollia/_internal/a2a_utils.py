"""Internal helpers used by the dispatch boundary to read A2A task fields.

The public surface relies on the ``@skill`` decorator plus the dispatch
glue in :mod:`apollia._internal.dispatch` to route tasks. Agent code
should not need to call these helpers directly - they live here so they
can be shared between dispatch primitives without leaking through the
public API.
"""

from __future__ import annotations

import json
from typing import Any

__all__ = ["extract_a2a_payload", "extract_skill_id"]


def _extract_parts(task: object) -> list[Any]:
    """Return the ``input.parts`` list of an AIP task, or an empty list."""
    if isinstance(task, dict):
        parts = task.get("input", {}).get("parts", [])
    else:
        # Tolerate AIPTask-like objects (dataclass/attr-based) without forcing
        # callers to know the concrete shape.
        input_obj = getattr(task, "input", None)
        parts = getattr(input_obj, "parts", []) if input_obj is not None else []
    return parts if isinstance(parts, list) else []


def _datapart_dict(part: object) -> dict[str, Any] | None:
    """Return the dict payload of a ``DataPart``, or ``None``."""
    if not isinstance(part, dict) or part.get("type") != "data":
        return None
    data = part.get("data")
    # mypy: cast through a fresh dict to drop the Any from .get()
    return dict(data) if isinstance(data, dict) else None


def _textpart_json_dict(part: object) -> dict[str, Any] | None:
    """Return the parsed JSON-object payload of a ``TextPart``, or ``None``."""
    if not isinstance(part, dict) or part.get("type") != "text":
        return None
    raw = (part.get("text") or "").strip()
    if not raw.startswith("{"):
        return None
    try:
        parsed = json.loads(raw)
    except json.JSONDecodeError:
        return None
    return parsed if isinstance(parsed, dict) else None


def extract_a2a_payload(task: object) -> dict[str, Any]:
    """Read the structured A2A payload from an AIP task.

    Prefers the canonical ``DataPart`` (what ``ctx.a2a.invoke`` actually
    sends). Falls back to parsing a ``TextPart`` that holds a JSON object,
    for the rare case where a worker is exercised directly with a textual
    JSON blob (tests, manual chat invocation, REST ``POST /api/v1/tasks``).

    Returns an empty dict when no usable payload is found - callers should
    treat that as "missing required fields" and return a domain error.
    """
    parts = _extract_parts(task)

    # 1. Prefer DataPart with a dict payload.
    for part in parts:
        payload = _datapart_dict(part)
        if payload is not None:
            return payload

    # 2. Fall back to a TextPart that holds a JSON object.
    for part in parts:
        payload = _textpart_json_dict(part)
        if payload is not None:
            return payload

    return {}


def extract_skill_id(task: object) -> str | None:
    """Return the A2A ``skill_id`` invoked for this task, or ``None``.

    Populated by the runtime when a task is dispatched through the A2A
    delegate (``ctx.a2a.invoke("chart.bar", payload)`` ⇒
    ``task["skill_id"] == "chart.bar"``). Workers that expose **multiple**
    skills must read this field to know which one was invoked - the
    payload alone is insufficient because ``AIPTask`` is the only carrier
    of routing information.

    Returns ``None`` for :

    * Root tasks (CLI ``agent run``, trigger fires, REST
      ``POST /api/v1/tasks``).
    * Chat-mode invocations (no skill targeting).
    * Older runtimes that do not yet propagate ``skill_id``.

    Convention : the value is the **full** skill identifier as registered
    in the agent's manifest (e.g. ``"chart.bar"``, ``"pdf.read-text"``),
    not just the suffix after the dot. Workers should match on full IDs.
    """
    value = task.get("skill_id") if isinstance(task, dict) else getattr(task, "skill_id", None)
    if isinstance(value, str) and value:
        return value
    return None

"""A2A-related helpers for agent code.

The Apollia runtime's A2A delegate (`crates/apollia-runtime/src/a2a/mod.rs`)
wraps the payload passed to `ctx.a2a_invoke(skill, payload)` in a single
`DataPart` — so the target worker receives:

    task["input"]["parts"] == [{"type": "data", "data": <payload>}]

Workers that look for a TextPart and try to parse JSON from `parts[i]["text"]`
silently fail (the worker bug fixed in veille-ia on 2026-05-16). Use
`extract_a2a_payload(task)` so this never happens again.
"""

from __future__ import annotations

import json
from typing import Any

__all__ = ["extract_a2a_payload", "extract_skill_id"]


def extract_a2a_payload(task: Any) -> dict[str, Any]:
    """Read the structured A2A payload from an AIP task.

    Prefers the canonical `DataPart` (what `ctx.a2a_invoke` actually sends).
    Falls back to parsing a `TextPart` that starts with `{` as JSON, for the
    rare case where a worker is exercised directly with a textual JSON blob
    (e.g. tests, manual chat invocation, REST `POST /api/v1/tasks`).

    Returns an empty dict when no usable payload is found — callers should
    treat that as "missing required fields" and return a domain error.
    """
    if isinstance(task, dict):
        parts = task.get("input", {}).get("parts", [])
    else:
        # Tolerate AIPTask-like objects (dataclass/attr-based) without forcing
        # callers to know the concrete shape.
        input_obj = getattr(task, "input", None)
        parts = getattr(input_obj, "parts", []) if input_obj is not None else []

    # 1. Prefer DataPart with a dict payload.
    for part in parts:
        if not isinstance(part, dict):
            continue
        if part.get("type") == "data" and isinstance(part.get("data"), dict):
            return part["data"]

    # 2. Fall back to a TextPart that holds a JSON object.
    for part in parts:
        if not isinstance(part, dict):
            continue
        if part.get("type") == "text":
            raw = (part.get("text") or "").strip()
            if raw.startswith("{"):
                try:
                    parsed = json.loads(raw)
                except json.JSONDecodeError:
                    continue
                if isinstance(parsed, dict):
                    return parsed

    return {}


def extract_skill_id(task: Any) -> str | None:
    """Return the A2A `skill_id` invoked for this task, or ``None``.

    Populated by the runtime when a task is dispatched through the A2A
    delegate (`ctx.a2a_invoke("chart.bar", payload)` → `task["skill_id"]
    == "chart.bar"`). Workers that expose **multiple** skills must read this
    field to know which one was invoked — the payload alone is insufficient
    because `AIPTask` is the only carrier of the routing information.

    Returns ``None`` for :
    - Root tasks (CLI `agent run`, trigger fires, REST `POST /api/v1/tasks`)
    - Chat-mode invocations (no skill targeting)
    - Older runtimes that do not yet propagate `skill_id`

    Convention : the value is the **full** skill identifier as registered in
    the agent's manifest (e.g. ``"chart.bar"``, ``"pdf.read-text"``), not just
    the suffix after the dot. Workers should match on full IDs.
    """
    if isinstance(task, dict):
        value = task.get("skill_id")
    else:
        value = getattr(task, "skill_id", None)
    if isinstance(value, str) and value:
        return value
    return None

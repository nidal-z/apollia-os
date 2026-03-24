"""Action parsing utilities for ReAct agents.

Extracts and validates JSON action objects from LLM responses, handling
markdown code fences and loose JSON embedded in free text.
"""

from __future__ import annotations

import json
import re
from typing import Any

# Action type constants matching the LLM output format.
ACTION_TOOL_CALL: str = "tool_call"
ACTION_FINAL_ANSWER: str = "final_answer"

# Matches a JSON object inside an optional ```json ... ``` fence.
_JSON_FENCE_RE = re.compile(r"```(?:json)?\s*(\{.*?\})\s*```", re.DOTALL)


class ActionParseError(Exception):
    """Raised when the LLM response cannot be parsed as a ReAct action."""


def extract_json(content: str) -> dict[str, Any]:
    """Extract the first JSON object from an LLM response.

    Tries three strategies in order:

    1. Parse the full content as JSON.
    2. Extract a ``\\`\\`\\`json ... \\`\\`\\`` fenced block.
    3. Find the outermost ``{ ... }`` span.

    Raises :class:`ActionParseError` when all strategies fail.
    """
    text = content.strip()

    # Strategy 1 — full content is already valid JSON.
    try:
        result = json.loads(text)
        if isinstance(result, dict):
            return result
    except (json.JSONDecodeError, ValueError):
        pass

    # Strategy 2 — JSON is inside a fenced block.
    match = _JSON_FENCE_RE.search(text)
    if match:
        try:
            result = json.loads(match.group(1))
            if isinstance(result, dict):
                return result
        except (json.JSONDecodeError, ValueError):
            pass

    # Strategy 3 — find outermost braces.
    start = text.find("{")
    end = text.rfind("}")
    if start != -1 and end > start:
        try:
            result = json.loads(text[start : end + 1])
            if isinstance(result, dict):
                return result
        except (json.JSONDecodeError, ValueError):
            pass

    raise ActionParseError(
        f"Could not extract a JSON action from: {text[:200]!r}"
    )


def validate_action(data: dict[str, Any]) -> dict[str, Any]:
    """Validate the structure of a parsed action dict.

    Expected shapes::

        {"thought": "...", "action": "tool_call",    "tool": "name", "args": {...}}
        {"thought": "...", "action": "final_answer", "text": "..."}

    Raises :class:`ActionParseError` on structural violations.
    """
    action_type = data.get("action")
    if action_type not in (ACTION_TOOL_CALL, ACTION_FINAL_ANSWER):
        raise ActionParseError(
            f"'action' must be '{ACTION_TOOL_CALL}' or '{ACTION_FINAL_ANSWER}', "
            f"got {action_type!r}"
        )

    if action_type == ACTION_TOOL_CALL and "tool" not in data:
        raise ActionParseError(
            "action 'tool_call' requires a 'tool' key"
        )

    if action_type == ACTION_FINAL_ANSWER and "text" not in data:
        raise ActionParseError(
            "action 'final_answer' requires a 'text' key"
        )

    data.setdefault("args", {})
    data.setdefault("thought", "")
    return data

"""Parsing utilities for Apollia agents.

Provides robust extraction of JSON, code blocks, and XML tags from LLM
responses, plus safe truncation.  All public functions handle empty or
malformed input gracefully — they return a sensible default instead of
raising.
"""

from __future__ import annotations

import json
import re
from typing import Any

# ---------------------------------------------------------------------------
# Action type constants matching the LLM output format.
# ---------------------------------------------------------------------------

ACTION_TOOL_CALL: str = "tool_call"
ACTION_FINAL_ANSWER: str = "final_answer"

# ---------------------------------------------------------------------------
# Pre-compiled patterns
# ---------------------------------------------------------------------------

# Matches a JSON object inside an optional ```json ... ``` fence.
_JSON_FENCE_RE = re.compile(r"```(?:json)?\s*(\{.*?\})\s*```", re.DOTALL)

# Matches a fenced code block with optional language tag (MULTILINE so ^ matches
# each line start; DOTALL so . matches newlines inside the block).
_CODE_BLOCK_RE = re.compile(r"^```(\w*)\n(.*?)^```", re.MULTILINE | re.DOTALL)

# ---------------------------------------------------------------------------
# Action-specific error (used by validate_action / ReAct loop)
# ---------------------------------------------------------------------------


class ActionParseError(Exception):
    """Raised when the LLM response cannot be parsed as a ReAct action."""


# ---------------------------------------------------------------------------
# Public API — general-purpose parsing
# ---------------------------------------------------------------------------


def extract_json(content: str) -> dict[str, Any]:
    """Extract the first JSON object from *content*.

    Tries three strategies in order:

    1. Parse the full content as JSON.
    2. Extract a fenced ``\\`\\`\\`json … \\`\\`\\``` block.
    3. Find the outermost ``{ … }`` span.

    Returns:
        Parsed dict, or empty dict if no valid JSON found.
    """
    if not content:
        return {}

    text = content.strip()
    if not text:
        return {}

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

    return {}


def extract_code_block(content: str, language: str = "") -> str | None:
    """Extract the first code block from markdown-formatted *content*.

    Args:
        content: Text potentially containing fenced code blocks.
        language: Optional language filter (e.g. ``"python"``, ``"bash"``).
                  An empty string matches any code block.

    Returns:
        Code block content (without fences), or ``None`` if not found.
    """
    if not content:
        return None

    for match in _CODE_BLOCK_RE.finditer(content):
        block_lang = match.group(1)
        if language and block_lang != language:
            continue
        return match.group(2)

    return None


def extract_xml_tag(content: str, tag: str) -> str | None:
    """Extract content between ``<tag>…</tag>`` from *content*.

    Args:
        content: Text potentially containing XML-style tags.
        tag: Tag name to extract (without angle brackets).

    Returns:
        Content between the opening and closing tags, or ``None``.
    """
    if not content or not tag:
        return None

    pattern = re.compile(rf"<{re.escape(tag)}>(.*?)</{re.escape(tag)}>", re.DOTALL)
    match = pattern.search(content)
    if match:
        return match.group(1)
    return None


def safe_json_loads(text: str, default: Any = None) -> Any:
    """Parse a JSON string, returning *default* on failure.

    Unlike :func:`json.loads`, this function never raises.  Useful for
    parsing potentially malformed LLM outputs.

    Args:
        text: JSON string to parse.
        default: Value returned when parsing fails.  Defaults to ``None``.

    Returns:
        Parsed JSON value, or *default*.
    """
    if not text:
        return default
    try:
        return json.loads(text)
    except (json.JSONDecodeError, ValueError, TypeError):
        return default


def truncate(text: str, max_chars: int = 2000, marker: str = "...") -> str:
    """Truncate *text* to at most *max_chars* characters.

    The *marker* is appended when the text is shortened and is included
    in the *max_chars* budget.  Python strings are Unicode sequences so
    slicing never splits a character.

    Args:
        text: Text to truncate.
        max_chars: Maximum length including marker.  Must be ``> len(marker)``.
        marker: String appended when text is truncated.

    Returns:
        Original text if within limit, or truncated text with marker.
    """
    if not text or len(text) <= max_chars:
        return text

    cut = max_chars - len(marker)
    if cut <= 0:
        return marker[:max_chars]

    return text[:cut] + marker


# ---------------------------------------------------------------------------
# Action-specific helpers (used by the ReAct agent loop)
# ---------------------------------------------------------------------------


def validate_action(data: dict[str, Any]) -> dict[str, Any]:
    """Validate the structure of a parsed action dict.

    Expected shapes::

        {"thought": "…", "action": "tool_call",    "tool": "name", "args": {…}}
        {"thought": "…", "action": "final_answer", "text": "…"}

    Raises:
        ActionParseError: On structural violations.
    """
    action_type = data.get("action")
    if action_type not in (ACTION_TOOL_CALL, ACTION_FINAL_ANSWER):
        raise ActionParseError(
            f"'action' must be '{ACTION_TOOL_CALL}' or '{ACTION_FINAL_ANSWER}', "
            f"got {action_type!r}"
        )

    if action_type == ACTION_TOOL_CALL and "tool" not in data:
        raise ActionParseError("action 'tool_call' requires a 'tool' key")

    if action_type == ACTION_FINAL_ANSWER and "text" not in data:
        raise ActionParseError("action 'final_answer' requires a 'text' key")

    data.setdefault("args", {})
    data.setdefault("thought", "")
    return data

"""Parsing utilities for Apollia agents.

Provides robust extraction of JSON, code blocks, and XML tags from LLM
responses, plus safe truncation.  All public functions handle empty or
malformed input gracefully - they return a sensible default instead of
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
# NOSONAR S5857 - lazy quantifier is needed to handle nested JSON objects;
# the `[^}]*` alternative would break matching of fenced nested objects.
_JSON_FENCE_RE = re.compile(r"```(?:json)?\s*(\{.*?\})\s*```", re.DOTALL)  # NOSONAR

# Matches a closed `<think>...</think>` reasoning block emitted by Qwen3,
# DeepSeek-R1 and similar thinking models. We strip these *before* JSON
# extraction so the inner reasoning prose can't be confused for the action.
_THINK_BLOCK_RE = re.compile(r"<think>.*?</think>", re.DOTALL | re.IGNORECASE)

# Matches a fenced code block with optional language tag (MULTILINE so ^ matches
# each line start; DOTALL so . matches newlines inside the block).
_CODE_BLOCK_RE = re.compile(r"^```(\w*)\n(.*?)^```", re.MULTILINE | re.DOTALL)

# ---------------------------------------------------------------------------
# Action-specific error (used by validate_action / ReAct loop)
# ---------------------------------------------------------------------------


class ActionParseError(Exception):
    """Raised when the LLM response cannot be parsed as a ReAct action."""


# ---------------------------------------------------------------------------
# Public API - general-purpose parsing
# ---------------------------------------------------------------------------


def _try_load_dict(text: str) -> dict[str, Any] | None:
    """Parse *text* as JSON and return it if (and only if) it's a dict."""
    try:
        result = json.loads(text)
    except ValueError:
        return None
    return result if isinstance(result, dict) else None


def _try_fenced_json(text: str) -> dict[str, Any] | None:
    """Return the dict inside a ```json …``` fence, or ``None``."""
    match = _JSON_FENCE_RE.search(text)
    if match is None:
        return None
    return _try_load_dict(match.group(1))


def _outermost_braces(text: str) -> str | None:
    """Return the ``{…}`` substring between the first ``{`` and last ``}``."""
    start = text.find("{")
    end = text.rfind("}")
    if start == -1 or end <= start:
        return None
    return text[start : end + 1]


def _try_repaired_json(target: str) -> dict[str, Any] | None:
    """Attempt the unescaped-quote repair on *target* and parse the result."""
    repaired = _repair_unescaped_quotes_in_long_strings(target)
    if repaired is None:
        return None
    return _try_load_dict(repaired)


def extract_json(content: str) -> dict[str, Any]:
    """Extract the first JSON object from *content*.

    Tries four strategies in order:

    1. Parse the full content as JSON.
    2. Extract a fenced ``\\`\\`\\`json … \\`\\`\\``` block.
    3. Find the outermost ``{ … }`` span.
    4. Attempt a heuristic repair of common LLM mistakes
       (unescaped quotes inside long ``content`` / ``new_text`` /
       ``old_text`` fields - classic failure when the LLM emits
       multi-line markdown inside a JSON string).

    Returns:
        Parsed dict, or empty dict if no valid JSON found.
    """
    if not content:
        return {}

    # Strip closed `<think>…</think>` reasoning blocks (Qwen3, DeepSeek-R1…)
    # before any extraction. An *unclosed* block (truncation mid-thinking)
    # is left untouched so downstream code still sees zero parseable JSON
    # and surfaces the error rather than silently returning `{}`.
    cleaned = _THINK_BLOCK_RE.sub("", content)
    text = cleaned.strip()
    if not text:
        return {}

    # Strategy 1 - full content is already valid JSON.
    result = _try_load_dict(text)
    if result is not None:
        return result

    # Strategy 2 - JSON is inside a fenced block.
    result = _try_fenced_json(text)
    if result is not None:
        return result

    # Strategy 3 - find outermost braces.
    candidate = _outermost_braces(text)
    if candidate is not None:
        result = _try_load_dict(candidate)
        if result is not None:
            return result

    # Strategy 4 - heuristic repair of long-string fields whose inner
    # quotes weren't escaped by the LLM.
    for target in (text, candidate):
        if not target:
            continue
        result = _try_repaired_json(target)
        if result is not None:
            return result

    return {}


# Fields commonly holding long multi-line content that LLMs fail to
# escape properly. Order matters - more specific first.
_LONG_STRING_FIELDS: tuple[str, ...] = (
    "content",  # file_write
    "new_text",  # file_edit
    "old_text",  # file_edit
    "text",  # final_answer
    "code",  # python_executor
)


def _repair_unescaped_quotes_in_long_strings(text: str) -> str | None:
    """Escape inner unescaped double quotes in known long-string fields.

    The LLM often emits multi-line markdown inside a JSON string without
    escaping the inner ``"``. We can't run a full JSON parser (that's
    what failed), but we can target the specific pattern:

        "<field>": "<content possibly with unescaped quotes>"

    by locating the opening ``"<field>": "`` and the closing ``"`` that's
    followed by ``,`` or ``}``  / ``]`` at the top level.

    Returns the repaired text, or ``None`` if no repairable pattern was
    found. The repair is best-effort - if it doesn't produce valid JSON
    the caller will simply fall back.
    """
    repaired = text
    repaired_any = False
    for field in _LONG_STRING_FIELDS:
        # Match the opening: `"field"\s*:\s*"`
        pattern = re.compile(
            r'"' + re.escape(field) + r'"\s*:\s*"',
        )
        m = pattern.search(repaired)
        if m is None:
            continue

        open_quote_pos = m.end() - 1  # position of the opening `"`
        # Find the closing quote of this string value.
        close_pos = _find_string_close(repaired, open_quote_pos)
        if close_pos is None:
            continue

        value = repaired[open_quote_pos + 1 : close_pos]
        escaped = _escape_inner_unescaped_quotes(value)
        if escaped == value:
            continue

        repaired = repaired[: open_quote_pos + 1] + escaped + repaired[close_pos:]
        repaired_any = True

    return repaired if repaired_any else None


def _find_string_close(text: str, open_quote_pos: int) -> int | None:
    """Locate the logical closing ``"`` of the string opened at *open_quote_pos*.

    Scans forward, ignoring ``\\"`` escapes. The closing is the first
    ``"`` followed (possibly after whitespace) by ``,``, ``}``, or ``]``
    - any other following character means we're still inside the value
    and the LLM forgot to escape this ``"``.
    """
    i = open_quote_pos + 1
    n = len(text)
    while i < n:
        ch = text[i]
        if ch == "\\":
            i += 2  # skip any escaped char
            continue
        if ch == '"':
            # Look ahead for terminating punctuation.
            j = i + 1
            while j < n and text[j].isspace():
                j += 1
            if j >= n or text[j] in ",}]":
                return i
        i += 1
    return None


def _escape_inner_unescaped_quotes(value: str) -> str:
    """Escape every ``"`` in *value* that isn't already preceded by ``\\``.

    ``value`` is the raw substring between the opening and closing quotes
    of a JSON string that failed to parse. We turn ``"`` into ``\\"``
    wherever the character itself is literal (not part of an escape).
    """
    out: list[str] = []
    i = 0
    n = len(value)
    while i < n:
        ch = value[i]
        if ch == "\\" and i + 1 < n:
            # Preserve existing escape pair.
            out.append(ch)
            out.append(value[i + 1])
            i += 2
            continue
        if ch == '"':
            out.append('\\"')
        else:
            out.append(ch)
        i += 1
    return "".join(out)


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
    except (ValueError, TypeError):
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
    """Validate and normalise a parsed ReAct action dict.

    Accepts two forms:

    Canonical::

        {"thought": "…", "action": "tool_call",    "tool": "name", "args": {…}}
        {"thought": "…", "action": "final_answer", "text": "…"}

    Shorthand (common LLM shortcut) - ``action`` carries the tool name
    directly, there is no ``tool`` field::

        {"thought": "…", "action": "ask_user", "args": {…}}

    When the shorthand is detected (``action`` is neither ``tool_call``
    nor ``final_answer``, and ``tool`` is absent), the input is rewritten
    to the canonical form with ``action="tool_call"`` and ``tool=<shorthand
    action value>``. This guard exists because LLMs frequently collapse
    the two fields - strict validation would crash the ReAct loop on an
    otherwise valid tool call.

    Raises:
        ActionParseError: On structural violations that cannot be
        recovered (missing action, missing text on final_answer, …).
    """
    action_type = data.get("action")

    # Shorthand: action is a tool name (not one of the reserved values)
    # AND no explicit `tool` field is set → rewrite to canonical form.
    if (
        isinstance(action_type, str)
        and action_type not in (ACTION_TOOL_CALL, ACTION_FINAL_ANSWER)
        and "tool" not in data
    ):
        data["tool"] = action_type
        data["action"] = ACTION_TOOL_CALL
        action_type = ACTION_TOOL_CALL

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

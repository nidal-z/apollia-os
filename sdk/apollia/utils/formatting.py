"""Output formatting utilities for Apollia agents.

Provides standardised conversion of arbitrary data into human-readable
text, Markdown tables, or indented JSON.  Also provides helpers to
extract plain text from AIPResult / AIPInput ``parts`` lists.

All public functions are pure (no side-effects) and depend only on the
standard library (``json``).
"""

from __future__ import annotations

import json
from typing import Any


def format_as_text(data: Any) -> str:
    """Convert an arbitrary value to a human-readable text representation.

    Args:
        data: Value to format.  ``str`` is returned as-is; ``dict`` is
            rendered as one ``key: value`` line per entry; ``list`` is
            rendered as one element per line; everything else goes
            through ``str()``.

    Returns:
        Plain-text representation of *data*.
    """
    if isinstance(data, str):
        return data

    if isinstance(data, dict):
        lines: list[str] = []
        for key, value in data.items():
            if isinstance(value, (dict, list)):
                lines.append(f"{key}:")
                nested = format_as_text(value)
                for nested_line in nested.splitlines():
                    lines.append(f"  {nested_line}")
            else:
                lines.append(f"{key}: {value}")
        return "\n".join(lines)

    if isinstance(data, list):
        return "\n".join(format_as_text(item) for item in data)

    return str(data)


def format_as_markdown(data: Any) -> str:
    """Convert a value to Markdown.

    * ``dict`` → two-column table (Key | Value).
    * ``list[dict]`` → multi-column table using the union of all keys.
    * Anything else → delegates to :func:`format_as_text`.

    Args:
        data: Value to format.

    Returns:
        Markdown representation of *data*.
    """
    if isinstance(data, dict):
        return _dict_to_markdown_table(data)

    if isinstance(data, list) and data and all(isinstance(item, dict) for item in data):
        return _list_of_dicts_to_markdown_table(data)

    return format_as_text(data)


def format_as_json(data: Any, indent: int = 2) -> str:
    """Serialise a value as indented JSON.

    Non-serialisable types are coerced via ``str()`` so that the call
    never raises for ``data`` that contains dates, sets, etc.

    Args:
        data: Value to serialise.
        indent: Number of spaces for indentation (default ``2``).

    Returns:
        JSON string.

    Raises:
        TypeError: Only if ``str()`` itself fails on a nested object
            (extremely unlikely).
    """
    return json.dumps(data, indent=indent, ensure_ascii=False, default=str)


def aip_result_text(result: dict[str, Any]) -> str:
    """Extract concatenated text from an AIPResult dictionary.

    Reads from the canonical ``output`` field (Rust shape).  For backward
    compatibility also reads from the legacy ``parts`` field.  Within each
    part, accepts either the canonical ``text`` key or the legacy
    ``content`` key.

    Args:
        result: AIPResult dictionary.

    Returns:
        Newline-joined text of all parts whose ``type`` is ``"text"``.
        Empty string when no text parts exist.
    """
    parts = result.get("output") or result.get("parts") or []
    return parts_to_text(parts)


def a2a_result_data(envelope: dict[str, Any]) -> dict[str, Any] | None:
    """Unwrap the skill payload from an ``ctx.a2a.invoke`` envelope.

    ``invoke`` returns the full A2A envelope
    (``{"result": {"output": [...], ...}, "agent_name", "skill_id",
    "duration_ms"}``). The skill's returned dict lives at the first ``data``
    part of ``result.output``. This helper digs it out.

    Args:
        envelope: The dict returned by ``ctx.a2a.invoke``.

    Returns:
        The ``data`` dict of the first ``type == "data"`` output part, or
        ``None`` when the call failed, produced no data part, or the shape is
        unexpected.
    """
    if not isinstance(envelope, dict):
        return None
    # Accept both the envelope ({"result": {...}}) and a bare AIPResult
    # ({"output": [...]}) so callers can pass either level.
    result = envelope.get("result")
    if not isinstance(result, dict):
        result = envelope
    output = result.get("output")
    if not isinstance(output, list):
        return None
    for part in output:
        if isinstance(part, dict) and part.get("type") == "data":
            data = part.get("data")
            return data if isinstance(data, dict) else None
    return None


def parts_to_text(parts: list[dict[str, Any]]) -> str:
    """Extract concatenated text from a list of AIP parts.

    Args:
        parts: List of dicts with ``type`` and (``text`` | ``content``) keys.

    Returns:
        Newline-joined text of all parts whose ``type`` is ``"text"``.
        Empty string when the list is empty or contains no text parts.
    """
    texts: list[str] = []
    for part in parts:
        if not isinstance(part, dict) or part.get("type") != "text":
            continue
        # Canonical key is "text" (mirrors Rust AIPPart). Legacy fallback
        # is "content" for older fixtures still living in the codebase.
        value = part.get("text")
        if value is None:
            value = part.get("content", "")
        texts.append(value)
    return "\n".join(texts)


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------


def _dict_to_markdown_table(data: dict[str, Any]) -> str:
    """Render a flat dict as a two-column Markdown table."""
    if not data:
        return "| Key | Value |\n| --- | --- |"

    rows = [
        f"| {_escape_pipe(str(key))} | {_escape_pipe(str(value))} |" for key, value in data.items()
    ]
    header = "| Key | Value |"
    separator = "| --- | --- |"
    return "\n".join([header, separator, *rows])


def _list_of_dicts_to_markdown_table(data: list[dict[str, Any]]) -> str:
    """Render a list of dicts as a multi-column Markdown table."""
    all_keys: list[str] = []
    seen: set[str] = set()
    for item in data:
        for key in item:
            if key not in seen:
                all_keys.append(key)
                seen.add(key)

    header = "| " + " | ".join(_escape_pipe(str(k)) for k in all_keys) + " |"
    separator = "| " + " | ".join("---" for _ in all_keys) + " |"

    rows: list[str] = []
    for item in data:
        cells = [_escape_pipe(str(item.get(k, ""))) for k in all_keys]
        rows.append("| " + " | ".join(cells) + " |")

    return "\n".join([header, separator, *rows])


def _escape_pipe(text: str) -> str:
    """Escape pipe characters so they don't break Markdown tables."""
    return text.replace("|", "\\|")

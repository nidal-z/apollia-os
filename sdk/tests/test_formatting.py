"""Tests for apollia.utils.formatting - text, Markdown, JSON, and AIP helpers."""

from __future__ import annotations

import json

from apollia.utils.formatting import (
    a2a_result_data,
    aip_result_text,
    format_as_json,
    format_as_markdown,
    format_as_text,
    parts_to_text,
)

# ---------------------------------------------------------------------------
# format_as_text
# ---------------------------------------------------------------------------


def test_format_as_text_with_dict() -> None:
    """Renders each key-value pair on its own line."""
    # GIVEN a flat mapping
    # WHEN it is rendered as text
    result = format_as_text({"nom": "Alice", "age": 30})

    # THEN every pair appears on a line of its own
    assert "nom: Alice" in result
    assert "age: 30" in result


def test_format_as_text_with_string() -> None:
    """Returns the string unchanged."""
    # GIVEN a value that is already text
    # WHEN it is rendered as text
    # THEN it comes back untouched, with no quoting or wrapping
    assert format_as_text("hello world") == "hello world"


def test_format_as_text_with_nested_structure() -> None:
    """Handles nested dicts and lists without raising."""
    # GIVEN a mapping holding a nested mapping and a nested list
    data = {"user": {"name": "Bob", "roles": ["admin", "dev"]}, "count": 3}

    # WHEN it is rendered as text
    result = format_as_text(data)

    # THEN every leaf is reachable in the output, at any depth
    assert "user:" in result
    assert "name: Bob" in result
    assert "admin" in result
    assert "dev" in result
    assert "count: 3" in result


def test_format_as_text_with_list() -> None:
    """Renders each list element on its own line."""
    # GIVEN a flat list
    # WHEN it is rendered as text
    result = format_as_text(["apple", "banana", "cherry"])

    # THEN one element per line, in order, with no bullet decoration
    lines = result.splitlines()
    assert lines == ["apple", "banana", "cherry"]


def test_format_as_text_with_number() -> None:
    """Coerces non-string scalars via str()."""
    # GIVEN numeric scalars
    # WHEN each is rendered as text
    # THEN str() is what decides the rendering
    assert format_as_text(42) == "42"
    assert format_as_text(3.14) == "3.14"


def test_format_as_text_with_none() -> None:
    """Coerces None via str()."""
    # GIVEN a None value
    # WHEN it is rendered as text
    # THEN it renders as "None" rather than as an empty string
    assert format_as_text(None) == "None"


# ---------------------------------------------------------------------------
# format_as_markdown
# ---------------------------------------------------------------------------


def test_format_as_markdown_dict_table() -> None:
    """Renders a dict as a two-column Markdown table."""
    # GIVEN a flat mapping
    # WHEN it is rendered as Markdown
    result = format_as_markdown({"nom": "Alice", "age": 30})

    # THEN a two-column table comes out, header and separator included
    assert "| Key | Value |" in result
    assert "| --- | --- |" in result
    assert "| nom | Alice |" in result
    assert "| age | 30 |" in result


def test_format_as_markdown_list_of_dicts() -> None:
    """Renders a list of dicts as a multi-column Markdown table."""
    # GIVEN a list of mappings sharing their keys
    data = [{"a": 1, "b": 2}, {"a": 3, "b": 4}]

    # WHEN it is rendered as Markdown
    result = format_as_markdown(data)

    # THEN the keys become columns and each mapping becomes a row
    assert "| a | b |" in result
    assert "| --- | --- |" in result
    assert "| 1 | 2 |" in result
    assert "| 3 | 4 |" in result


def test_format_as_markdown_empty_dict() -> None:
    """Renders an empty dict as headers only."""
    # GIVEN an empty mapping
    # WHEN it is rendered as Markdown
    result = format_as_markdown({})

    # THEN the table header still comes out, so the shape stays valid Markdown
    assert "| Key | Value |" in result
    assert "| --- | --- |" in result


def test_format_as_markdown_non_dict() -> None:
    """Falls back to format_as_text for non-dict/non-list-of-dicts."""
    # GIVEN values that cannot become a table
    # WHEN each is rendered as Markdown
    # THEN the text renderer takes over instead of emitting an empty table
    assert format_as_markdown("plain text") == "plain text"
    assert format_as_markdown(42) == "42"


def test_format_as_markdown_pipe_in_value() -> None:
    """Escapes pipe characters so they don't break the table."""
    # GIVEN a value containing the column separator
    # WHEN it is rendered as Markdown
    result = format_as_markdown({"cmd": "a | b"})

    # THEN the pipe is escaped, so the row keeps its two columns
    assert "a \\| b" in result


# ---------------------------------------------------------------------------
# format_as_json
# ---------------------------------------------------------------------------


def test_format_as_json_indent() -> None:
    """Serialises with the requested indentation."""
    # GIVEN a mapping and an explicit indent of four spaces
    data = {"key": "value"}

    # WHEN it is serialised
    result = format_as_json(data, indent=4)

    # THEN the output parses back to the input and carries the requested indent
    parsed = json.loads(result)
    assert parsed == data
    assert "\n    " in result


def test_format_as_json_default_indent() -> None:
    """Uses 2-space indent by default."""
    # GIVEN a mapping and no indent argument
    # WHEN it is serialised
    result = format_as_json({"a": 1})

    # THEN the output is indented by two spaces, not compact
    assert "\n  " in result


def test_format_as_json_non_ascii() -> None:
    """Preserves non-ASCII characters (ensure_ascii=False)."""
    # GIVEN a value carrying a non-ASCII character
    # WHEN it is serialised
    result = format_as_json({"ville": "Zürich"})

    # THEN the character survives verbatim instead of becoming an escape
    assert "Zürich" in result


def test_format_as_json_non_serialisable() -> None:
    """Falls back to str() for non-serialisable types."""
    # GIVEN a value JSON cannot represent, a set
    data = {"items": {1, 2, 3}}

    # WHEN it is serialised
    result = format_as_json(data)

    # THEN the output is still parseable JSON, so the helper never raises
    parsed = json.loads(result)
    assert "items" in parsed


# ---------------------------------------------------------------------------
# aip_result_text
# ---------------------------------------------------------------------------


def test_aip_result_text_extracts_content() -> None:
    """Extracts text parts and ignores non-text parts."""
    # GIVEN a result carrying two text parts and one data part
    result_dict = {
        "status": "completed",
        "parts": [
            {"type": "text", "content": "Hello world"},
            {"type": "data", "content": "binary stuff"},
            {"type": "text", "content": "Second line"},
        ],
    }

    # WHEN the text is extracted
    text = aip_result_text(result_dict)

    # THEN only the text parts are kept
    assert "Hello world" in text
    assert "Second line" in text
    assert "binary stuff" not in text


def test_aip_result_text_no_parts() -> None:
    """Returns empty string when parts key is missing."""
    # GIVEN a result with no "parts" key at all
    # WHEN the text is extracted
    # THEN an empty string comes back instead of a KeyError
    assert aip_result_text({"status": "completed"}) == ""


def test_aip_result_text_no_text_parts() -> None:
    """Returns empty string when no part has type 'text'."""
    # GIVEN a result whose only part is data
    result_dict = {
        "parts": [{"type": "data", "content": "only data"}],
    }

    # WHEN the text is extracted
    # THEN an empty string comes back, not the data rendered as text
    assert aip_result_text(result_dict) == ""


# ---------------------------------------------------------------------------
# parts_to_text
# ---------------------------------------------------------------------------


def test_parts_to_text_empty_list() -> None:
    """Returns empty string for an empty list."""
    # GIVEN no part at all
    # WHEN the parts are joined
    # THEN an empty string comes back, with no stray newline
    assert parts_to_text([]) == ""


def test_parts_to_text_mixed() -> None:
    """Concatenates only text parts with newlines."""
    # GIVEN two text parts with an image part between them
    parts = [
        {"type": "text", "content": "line 1"},
        {"type": "image", "content": "img_data"},
        {"type": "text", "content": "line 2"},
    ]

    # WHEN the parts are joined
    result = parts_to_text(parts)

    # THEN the image is skipped and the two texts are joined by one newline
    assert result == "line 1\nline 2"


def test_parts_to_text_non_dict_items() -> None:
    """Silently skips non-dict items in the list."""
    # GIVEN a list mixing well-formed parts with a string and an int
    parts = [
        {"type": "text", "content": "ok"},
        "not a dict",
        42,
        {"type": "text", "content": "also ok"},
    ]

    # WHEN the parts are joined
    result = parts_to_text(parts)  # type: ignore[arg-type]

    # THEN the malformed entries are skipped rather than raising
    assert result == "ok\nalso ok"


# ---------------------------------------------------------------------------
# a2a_result_data
# ---------------------------------------------------------------------------


def test_a2a_result_data_unwraps_envelope() -> None:
    """Digs the skill payload out of the full a2a invoke envelope."""
    # GIVEN the full envelope an a2a invoke returns
    envelope = {
        "result": {
            "task_id": "t1",
            "status": "completed",
            "output": [{"type": "data", "data": {"score": 42}}],
        },
        "agent_name": "worker",
        "skill_id": "classify",
        "duration_ms": 12,
    }

    # WHEN the data is extracted
    # THEN the skill payload comes back, stripped of both wrappers
    assert a2a_result_data(envelope) == {"score": 42}


def test_a2a_result_data_accepts_bare_result() -> None:
    """Accepts a bare AIPResult dict (no `result` wrapper)."""
    # GIVEN a bare result with no envelope around it
    bare = {"output": [{"type": "data", "data": {"ok": True}}]}

    # WHEN the data is extracted
    # THEN the payload still comes back, so both shapes are accepted
    assert a2a_result_data(bare) == {"ok": True}


def test_a2a_result_data_returns_none_on_failure() -> None:
    """Returns None when there is no data part or the call failed."""
    # GIVEN envelopes with no data part, and inputs that are not envelopes
    # WHEN the data is extracted from each
    # THEN None comes back every time, rather than a partial payload
    assert a2a_result_data({"result": {"status": "failed", "output": []}}) is None
    assert a2a_result_data({"result": {"output": [{"type": "text", "text": "hi"}]}}) is None
    assert a2a_result_data({}) is None
    assert a2a_result_data("not a dict") is None  # type: ignore[arg-type]

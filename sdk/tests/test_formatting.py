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
    result = format_as_text({"nom": "Alice", "age": 30})
    assert "nom: Alice" in result
    assert "age: 30" in result


def test_format_as_text_with_string() -> None:
    """Returns the string unchanged."""
    assert format_as_text("hello world") == "hello world"


def test_format_as_text_with_nested_structure() -> None:
    """Handles nested dicts and lists without raising."""
    data = {"user": {"name": "Bob", "roles": ["admin", "dev"]}, "count": 3}
    result = format_as_text(data)
    assert "user:" in result
    assert "name: Bob" in result
    assert "admin" in result
    assert "dev" in result
    assert "count: 3" in result


def test_format_as_text_with_list() -> None:
    """Renders each list element on its own line."""
    result = format_as_text(["apple", "banana", "cherry"])
    lines = result.splitlines()
    assert lines == ["apple", "banana", "cherry"]


def test_format_as_text_with_number() -> None:
    """Coerces non-string scalars via str()."""
    assert format_as_text(42) == "42"
    assert format_as_text(3.14) == "3.14"


def test_format_as_text_with_none() -> None:
    """Coerces None via str()."""
    assert format_as_text(None) == "None"


# ---------------------------------------------------------------------------
# format_as_markdown
# ---------------------------------------------------------------------------


def test_format_as_markdown_dict_table() -> None:
    """Renders a dict as a two-column Markdown table."""
    result = format_as_markdown({"nom": "Alice", "age": 30})
    assert "| Key | Value |" in result
    assert "| --- | --- |" in result
    assert "| nom | Alice |" in result
    assert "| age | 30 |" in result


def test_format_as_markdown_list_of_dicts() -> None:
    """Renders a list of dicts as a multi-column Markdown table."""
    data = [{"a": 1, "b": 2}, {"a": 3, "b": 4}]
    result = format_as_markdown(data)
    assert "| a | b |" in result
    assert "| --- | --- |" in result
    assert "| 1 | 2 |" in result
    assert "| 3 | 4 |" in result


def test_format_as_markdown_empty_dict() -> None:
    """Renders an empty dict as headers only."""
    result = format_as_markdown({})
    assert "| Key | Value |" in result
    assert "| --- | --- |" in result


def test_format_as_markdown_non_dict() -> None:
    """Falls back to format_as_text for non-dict/non-list-of-dicts."""
    assert format_as_markdown("plain text") == "plain text"
    assert format_as_markdown(42) == "42"


def test_format_as_markdown_pipe_in_value() -> None:
    """Escapes pipe characters so they don't break the table."""
    result = format_as_markdown({"cmd": "a | b"})
    assert "a \\| b" in result


# ---------------------------------------------------------------------------
# format_as_json
# ---------------------------------------------------------------------------


def test_format_as_json_indent() -> None:
    """Serialises with the requested indentation."""
    data = {"key": "value"}
    result = format_as_json(data, indent=4)
    parsed = json.loads(result)
    assert parsed == data
    assert "\n    " in result


def test_format_as_json_default_indent() -> None:
    """Uses 2-space indent by default."""
    result = format_as_json({"a": 1})
    assert "\n  " in result


def test_format_as_json_non_ascii() -> None:
    """Preserves non-ASCII characters (ensure_ascii=False)."""
    result = format_as_json({"ville": "Zürich"})
    assert "Zürich" in result


def test_format_as_json_non_serialisable() -> None:
    """Falls back to str() for non-serialisable types."""
    data = {"items": {1, 2, 3}}
    result = format_as_json(data)
    parsed = json.loads(result)
    assert "items" in parsed


# ---------------------------------------------------------------------------
# aip_result_text
# ---------------------------------------------------------------------------


def test_aip_result_text_extracts_content() -> None:
    """Extracts text parts and ignores non-text parts."""
    result_dict = {
        "status": "completed",
        "parts": [
            {"type": "text", "content": "Hello world"},
            {"type": "data", "content": "binary stuff"},
            {"type": "text", "content": "Second line"},
        ],
    }
    text = aip_result_text(result_dict)
    assert "Hello world" in text
    assert "Second line" in text
    assert "binary stuff" not in text


def test_aip_result_text_no_parts() -> None:
    """Returns empty string when parts key is missing."""
    assert aip_result_text({"status": "completed"}) == ""


def test_aip_result_text_no_text_parts() -> None:
    """Returns empty string when no part has type 'text'."""
    result_dict = {
        "parts": [{"type": "data", "content": "only data"}],
    }
    assert aip_result_text(result_dict) == ""


# ---------------------------------------------------------------------------
# parts_to_text
# ---------------------------------------------------------------------------


def test_parts_to_text_empty_list() -> None:
    """Returns empty string for an empty list."""
    assert parts_to_text([]) == ""


def test_parts_to_text_mixed() -> None:
    """Concatenates only text parts with newlines."""
    parts = [
        {"type": "text", "content": "line 1"},
        {"type": "image", "content": "img_data"},
        {"type": "text", "content": "line 2"},
    ]
    result = parts_to_text(parts)
    assert result == "line 1\nline 2"


def test_parts_to_text_non_dict_items() -> None:
    """Silently skips non-dict items in the list."""
    parts = [
        {"type": "text", "content": "ok"},
        "not a dict",
        42,
        {"type": "text", "content": "also ok"},
    ]
    result = parts_to_text(parts)  # type: ignore[arg-type]
    assert result == "ok\nalso ok"


# ---------------------------------------------------------------------------
# a2a_result_data
# ---------------------------------------------------------------------------


def test_a2a_result_data_unwraps_envelope() -> None:
    """Digs the skill payload out of the full a2a invoke envelope."""
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
    assert a2a_result_data(envelope) == {"score": 42}


def test_a2a_result_data_accepts_bare_result() -> None:
    """Accepts a bare AIPResult dict (no `result` wrapper)."""
    bare = {"output": [{"type": "data", "data": {"ok": True}}]}
    assert a2a_result_data(bare) == {"ok": True}


def test_a2a_result_data_returns_none_on_failure() -> None:
    """Returns None when there is no data part or the call failed."""
    assert a2a_result_data({"result": {"status": "failed", "output": []}}) is None
    assert a2a_result_data({"result": {"output": [{"type": "text", "text": "hi"}]}}) is None
    assert a2a_result_data({}) is None
    assert a2a_result_data("not a dict") is None  # type: ignore[arg-type]

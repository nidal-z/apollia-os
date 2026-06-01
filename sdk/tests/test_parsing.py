"""Tests for apollia.utils.parsing - JSON, code block, XML, and truncate utilities."""

from __future__ import annotations

from apollia.utils.parsing import (
    extract_code_block,
    extract_json,
    extract_xml_tag,
    safe_json_loads,
    truncate,
)


# ---------------------------------------------------------------------------
# extract_json
# ---------------------------------------------------------------------------


def test_extract_json_from_code_block() -> None:
    """Finds JSON inside a markdown code block."""
    content = '```json\n{"action": "bash", "input": "ls"}\n```'
    result = extract_json(content)
    assert result["action"] == "bash"
    assert result["input"] == "ls"


def test_extract_json_raw() -> None:
    """Finds raw JSON embedded in text."""
    content = 'Here is the result: {"status": "ok"} end.'
    result = extract_json(content)
    assert result["status"] == "ok"


def test_extract_json_full_content() -> None:
    """Parses content that is pure JSON."""
    result = extract_json('{"a": 1, "b": 2}')
    assert result == {"a": 1, "b": 2}


def test_extract_json_empty() -> None:
    """Returns empty dict on empty or non-JSON input."""
    assert extract_json("") == {}
    assert extract_json("no json here") == {}
    assert extract_json("   ") == {}


# ---------------------------------------------------------------------------
# extract_code_block
# ---------------------------------------------------------------------------


def test_extract_code_block_python() -> None:
    """Finds a python code block."""
    content = '```python\nprint("hello")\n```'
    result = extract_code_block(content, language="python")
    assert result is not None
    assert 'print("hello")' in result


def test_extract_code_block_any_language() -> None:
    """Finds any code block when no language filter is given."""
    content = "Some text\n```bash\nls -la\n```\nMore text"
    result = extract_code_block(content)
    assert result is not None
    assert "ls -la" in result


def test_extract_code_block_language_mismatch() -> None:
    """Returns None when the language filter does not match."""
    content = '```javascript\nconsole.log("hi")\n```'
    assert extract_code_block(content, language="python") is None


def test_extract_code_block_no_match() -> None:
    """Returns None when no code block is present."""
    assert extract_code_block("no code here") is None
    assert extract_code_block("") is None


# ---------------------------------------------------------------------------
# extract_xml_tag
# ---------------------------------------------------------------------------


def test_extract_xml_tag() -> None:
    """Extracts content between tags."""
    content = "Before <result>success</result> after"
    assert extract_xml_tag(content, "result") == "success"


def test_extract_xml_tag_multiline() -> None:
    """Extracts multiline content between tags."""
    content = "<response>\nline one\nline two\n</response>"
    result = extract_xml_tag(content, "response")
    assert result is not None
    assert "line one" in result
    assert "line two" in result


def test_extract_xml_tag_not_found() -> None:
    """Returns None when the tag is absent."""
    assert extract_xml_tag("no tags", "result") is None
    assert extract_xml_tag("", "result") is None


def test_extract_xml_tag_special_chars() -> None:
    """Escapes tag name so regex metacharacters are treated literally."""
    # "a.b" as tag should NOT match "<axb>val</axb>" (dot is literal, not wildcard)
    assert extract_xml_tag("<axb>val</axb>", "a.b") is None
    # But it should match the literal "<a.b>val</a.b>"
    assert extract_xml_tag("<a.b>val</a.b>", "a.b") == "val"


# ---------------------------------------------------------------------------
# safe_json_loads
# ---------------------------------------------------------------------------


def test_safe_json_loads_valid() -> None:
    """Parses valid JSON."""
    assert safe_json_loads('{"a": 1}') == {"a": 1}
    assert safe_json_loads("[1, 2, 3]") == [1, 2, 3]
    assert safe_json_loads('"hello"') == "hello"


def test_safe_json_loads_invalid() -> None:
    """Returns default on invalid or empty input."""
    assert safe_json_loads("not json", default={}) == {}
    assert safe_json_loads("", default=None) is None
    assert safe_json_loads("{broken", default=[]) == []


# ---------------------------------------------------------------------------
# truncate
# ---------------------------------------------------------------------------


def test_truncate_short_text() -> None:
    """Returns original text when within limit."""
    assert truncate("hello", max_chars=100) == "hello"


def test_truncate_exact_limit() -> None:
    """Returns original text when exactly at limit."""
    assert truncate("abcde", max_chars=5) == "abcde"


def test_truncate_long_text() -> None:
    """Cuts text and appends marker."""
    result = truncate("a" * 100, max_chars=10, marker="...")
    assert len(result) == 10
    assert result.endswith("...")
    assert result == "a" * 7 + "..."


def test_truncate_utf8() -> None:
    """Handles multi-byte characters correctly."""
    text = "\u00e9\u00e0\u00fc\u00f1\u00e7\u00e8\u00ea\u00eb\u00ef\u00f4"
    result = truncate(text, max_chars=8)
    assert len(result) <= 8


def test_truncate_empty() -> None:
    """Handles empty input."""
    assert truncate("", max_chars=10) == ""


def test_truncate_custom_marker() -> None:
    """Supports a custom marker string."""
    result = truncate("a" * 20, max_chars=10, marker=" [cut]")
    assert len(result) == 10
    assert result.endswith(" [cut]")

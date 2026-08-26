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
    # GIVEN a model answer wrapping its JSON in a fenced json block
    content = '```json\n{"action": "bash", "input": "ls"}\n```'

    # WHEN the JSON is extracted
    result = extract_json(content)

    # THEN the fence is stripped and the object is parsed
    assert result["action"] == "bash"
    assert result["input"] == "ls"


def test_extract_json_raw() -> None:
    """Finds raw JSON embedded in text."""
    # GIVEN prose with a JSON object in the middle of it
    content = 'Here is the result: {"status": "ok"} end.'

    # WHEN the JSON is extracted
    result = extract_json(content)

    # THEN the object is found despite the surrounding prose
    assert result["status"] == "ok"


def test_extract_json_full_content() -> None:
    """Parses content that is pure JSON."""
    # GIVEN a string that is nothing but a JSON object
    # WHEN the JSON is extracted
    result = extract_json('{"a": 1, "b": 2}')

    # THEN the whole object comes back
    assert result == {"a": 1, "b": 2}


def test_extract_json_empty() -> None:
    """Returns empty dict on empty or non-JSON input."""
    # GIVEN inputs that carry no JSON at all
    # WHEN the JSON is extracted from each
    # THEN an empty dict comes back instead of a parse error
    assert extract_json("") == {}
    assert extract_json("no json here") == {}
    assert extract_json("   ") == {}


# ---------------------------------------------------------------------------
# extract_code_block
# ---------------------------------------------------------------------------


def test_extract_code_block_python() -> None:
    """Finds a python code block."""
    # GIVEN an answer carrying a fenced python block
    content = '```python\nprint("hello")\n```'

    # WHEN the block is extracted with the python filter
    result = extract_code_block(content, language="python")

    # THEN the code inside the fence comes back
    assert result is not None
    assert 'print("hello")' in result


def test_extract_code_block_any_language() -> None:
    """Finds any code block when no language filter is given."""
    # GIVEN prose around a fenced bash block
    content = "Some text\n```bash\nls -la\n```\nMore text"

    # WHEN the block is extracted with no language filter
    result = extract_code_block(content)

    # THEN the block is found whatever its language tag
    assert result is not None
    assert "ls -la" in result


def test_extract_code_block_language_mismatch() -> None:
    """Returns None when the language filter does not match."""
    # GIVEN a fenced javascript block
    content = '```javascript\nconsole.log("hi")\n```'

    # WHEN the block is extracted with the python filter
    # THEN nothing comes back, rather than the wrong language
    assert extract_code_block(content, language="python") is None


def test_extract_code_block_no_match() -> None:
    """Returns None when no code block is present."""
    # GIVEN inputs carrying no fenced block
    # WHEN a block is extracted from each
    # THEN None comes back instead of the raw text
    assert extract_code_block("no code here") is None
    assert extract_code_block("") is None


# ---------------------------------------------------------------------------
# extract_xml_tag
# ---------------------------------------------------------------------------


def test_extract_xml_tag() -> None:
    """Extracts content between tags."""
    # GIVEN text carrying one tagged span
    content = "Before <result>success</result> after"

    # WHEN the tag is extracted
    # THEN the inner text comes back without the tags
    assert extract_xml_tag(content, "result") == "success"


def test_extract_xml_tag_multiline() -> None:
    """Extracts multiline content between tags."""
    # GIVEN a tagged span spread over several lines
    content = "<response>\nline one\nline two\n</response>"

    # WHEN the tag is extracted
    result = extract_xml_tag(content, "response")

    # THEN every line inside the span is carried
    assert result is not None
    assert "line one" in result
    assert "line two" in result


def test_extract_xml_tag_not_found() -> None:
    """Returns None when the tag is absent."""
    # GIVEN texts that carry no such tag
    # WHEN the tag is extracted from each
    # THEN None comes back instead of an empty string
    assert extract_xml_tag("no tags", "result") is None
    assert extract_xml_tag("", "result") is None


def test_extract_xml_tag_special_chars() -> None:
    """Escapes tag name so regex metacharacters are treated literally."""
    # GIVEN a tag name carrying a regex metacharacter
    # WHEN it is matched against a span that only a wildcard would match
    # THEN nothing comes back, because the dot is literal
    # "a.b" as tag should NOT match "<axb>val</axb>" (dot is literal, not wildcard)
    assert extract_xml_tag("<axb>val</axb>", "a.b") is None
    # But it should match the literal "<a.b>val</a.b>"
    assert extract_xml_tag("<a.b>val</a.b>", "a.b") == "val"


# ---------------------------------------------------------------------------
# safe_json_loads
# ---------------------------------------------------------------------------


def test_safe_json_loads_valid() -> None:
    """Parses valid JSON."""
    # GIVEN valid JSON of the three top-level shapes
    # WHEN each is parsed
    # THEN the parsed value comes back, not the default
    assert safe_json_loads('{"a": 1}') == {"a": 1}
    assert safe_json_loads("[1, 2, 3]") == [1, 2, 3]
    assert safe_json_loads('"hello"') == "hello"


def test_safe_json_loads_invalid() -> None:
    """Returns default on invalid or empty input."""
    # GIVEN inputs that are not parseable JSON, each with its own default
    # WHEN each is parsed
    # THEN the caller's default comes back instead of an exception
    assert safe_json_loads("not json", default={}) == {}
    assert safe_json_loads("", default=None) is None
    assert safe_json_loads("{broken", default=[]) == []


# ---------------------------------------------------------------------------
# truncate
# ---------------------------------------------------------------------------


def test_truncate_short_text() -> None:
    """Returns original text when within limit."""
    # GIVEN a text shorter than the limit
    # WHEN it is truncated
    # THEN it comes back untouched, with no marker appended
    assert truncate("hello", max_chars=100) == "hello"


def test_truncate_exact_limit() -> None:
    """Returns original text when exactly at limit."""
    # GIVEN a text exactly as long as the limit
    # WHEN it is truncated
    # THEN it comes back untouched, so the limit is inclusive
    assert truncate("abcde", max_chars=5) == "abcde"


def test_truncate_long_text() -> None:
    """Cuts text and appends marker."""
    # GIVEN a text far longer than the limit and a three-character marker
    # WHEN it is truncated
    result = truncate("a" * 100, max_chars=10, marker="...")

    # THEN the marker is counted inside the limit, not added on top of it
    assert len(result) == 10
    assert result.endswith("...")
    assert result == "a" * 7 + "..."


def test_truncate_utf8() -> None:
    """Handles multi-byte characters correctly."""
    # GIVEN a text made of multi-byte characters
    text = "\u00e9\u00e0\u00fc\u00f1\u00e7\u00e8\u00ea\u00eb\u00ef\u00f4"

    # WHEN it is truncated to eight characters
    result = truncate(text, max_chars=8)

    # THEN the limit is counted in characters, not in bytes
    assert len(result) <= 8


def test_truncate_empty() -> None:
    """Handles empty input."""
    # GIVEN an empty text
    # WHEN it is truncated
    # THEN it comes back empty, with no marker
    assert truncate("", max_chars=10) == ""


def test_truncate_custom_marker() -> None:
    """Supports a custom marker string."""
    # GIVEN a text over the limit and a caller-supplied marker
    # WHEN it is truncated
    result = truncate("a" * 20, max_chars=10, marker=" [cut]")

    # THEN the custom marker is used and still counted inside the limit
    assert len(result) == 10
    assert result.endswith(" [cut]")

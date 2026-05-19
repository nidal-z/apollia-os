"""Parsing and formatting utilities for Apollia agents."""

from apollia.utils.a2a import extract_a2a_payload
from apollia.utils.assertion import (
    AssertionSpec,
    Citation,
    ConfidenceLevel,
    SourceType,
    assert_with_confidence,
    build_citation_payload,
)
from apollia.utils.formatting import (
    aip_result_text,
    format_as_json,
    format_as_markdown,
    format_as_text,
    parts_to_text,
)
from apollia.utils.hitl import resume_pending_tool
from apollia.utils.parsing import (
    ActionParseError,
    extract_code_block,
    extract_json,
    extract_xml_tag,
    safe_json_loads,
    truncate,
    validate_action,
)

__all__ = [
    "ActionParseError",
    "AssertionSpec",
    "Citation",
    "ConfidenceLevel",
    "SourceType",
    "aip_result_text",
    "assert_with_confidence",
    "build_citation_payload",
    "extract_a2a_payload",
    "extract_code_block",
    "extract_json",
    "extract_xml_tag",
    "format_as_json",
    "format_as_markdown",
    "format_as_text",
    "parts_to_text",
    "resume_pending_tool",
    "safe_json_loads",
    "truncate",
    "validate_action",
]

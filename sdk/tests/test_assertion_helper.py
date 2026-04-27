"""Tests for apollia.utils.assertion (confidence and citation markup helper)."""

from __future__ import annotations

import pytest

from apollia.utils.assertion import (
    Citation,
    assert_with_confidence,
    build_citation_payload,
)


class TestAssertWithConfidence:
    def test_wraps_text_with_high_level(self) -> None:
        out = assert_with_confidence("Earth is round", level="high")
        assert out == "[conf:high]Earth is round[/conf]"

    def test_attaches_single_citation(self) -> None:
        c = Citation(id="nasa", title="NASA")
        out = assert_with_confidence("Earth is round", level="high", citations=[c])
        assert out == "[conf:high]Earth is round[cite:nasa][/conf]"

    def test_attaches_multiple_citations_in_order(self) -> None:
        out = assert_with_confidence(
            "claim",
            level="medium",
            citations=[Citation(id="a", title="A"), Citation(id="b", title="B")],
        )
        assert out == "[conf:medium]claim[cite:a][cite:b][/conf]"

    def test_rejects_invalid_level(self) -> None:
        with pytest.raises(ValueError, match="level must be one of"):
            assert_with_confidence("x", level="ultra")  # type: ignore[arg-type]

    def test_rejects_nested_markers(self) -> None:
        with pytest.raises(ValueError, match="already contain"):
            assert_with_confidence("[conf:high]x[/conf]", level="high")


class TestCitation:
    def test_to_dict_drops_none_fields(self) -> None:
        c = Citation(id="a", title="Title")
        assert c.to_dict() == {"id": "a", "title": "Title", "source_type": "other"}

    def test_to_dict_keeps_url_and_excerpt(self) -> None:
        c = Citation(
            id="a",
            title="T",
            url="https://example.com",
            excerpt="…",
            source_type="web",
        )
        assert c.to_dict() == {
            "id": "a",
            "title": "T",
            "url": "https://example.com",
            "excerpt": "…",
            "source_type": "web",
        }

    def test_rejects_empty_id(self) -> None:
        with pytest.raises(ValueError, match="non-empty"):
            Citation(id="  ", title="T")

    def test_rejects_forbidden_chars_in_id(self) -> None:
        with pytest.raises(ValueError, match="may not contain"):
            Citation(id="a,b", title="T")

    def test_rejects_unknown_source_type(self) -> None:
        with pytest.raises(ValueError, match="source_type"):
            Citation(id="a", title="T", source_type="podcast")  # type: ignore[arg-type]


class TestBuildCitationPayload:
    def test_deduplicates_by_id(self) -> None:
        payload = build_citation_payload(
            [
                Citation(id="a", title="A"),
                Citation(id="a", title="A-dup"),
                Citation(id="b", title="B"),
            ]
        )
        assert [c["id"] for c in payload] == ["a", "b"]

    def test_empty_iterable_returns_empty(self) -> None:
        assert build_citation_payload([]) == []

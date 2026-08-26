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
        # GIVEN a plain claim and the "high" confidence level
        # WHEN it is marked up
        out = assert_with_confidence("Earth is round", level="high")

        # THEN the text is wrapped in the level's opening and closing markers
        assert out == "[conf:high]Earth is round[/conf]"

    def test_attaches_single_citation(self) -> None:
        # GIVEN a claim and one citation
        c = Citation(id="nasa", title="NASA")

        # WHEN the claim is marked up with that citation
        out = assert_with_confidence("Earth is round", level="high", citations=[c])

        # THEN the citation marker sits inside the confidence span
        assert out == "[conf:high]Earth is round[cite:nasa][/conf]"

    def test_attaches_multiple_citations_in_order(self) -> None:
        # GIVEN a claim and two citations in a declared order
        # WHEN the claim is marked up with both
        out = assert_with_confidence(
            "claim",
            level="medium",
            citations=[Citation(id="a", title="A"), Citation(id="b", title="B")],
        )

        # THEN both markers appear, in the order they were given
        assert out == "[conf:medium]claim[cite:a][cite:b][/conf]"

    def test_rejects_invalid_level(self) -> None:
        # GIVEN a confidence level outside the accepted set
        # WHEN the claim is marked up
        # THEN the helper refuses instead of emitting an unknown marker
        with pytest.raises(ValueError, match="level must be one of"):
            assert_with_confidence("x", level="ultra")  # type: ignore[arg-type]

    def test_rejects_nested_markers(self) -> None:
        # GIVEN a text that already carries confidence markers
        # WHEN it is marked up again
        # THEN the helper refuses instead of nesting spans
        with pytest.raises(ValueError, match="already contain"):
            assert_with_confidence("[conf:high]x[/conf]", level="high")


class TestCitation:
    def test_to_dict_drops_none_fields(self) -> None:
        # GIVEN a citation with only the required fields
        c = Citation(id="a", title="Title")

        # WHEN it is serialised
        # THEN the unset fields are absent and source_type falls back to "other"
        assert c.to_dict() == {"id": "a", "title": "Title", "source_type": "other"}

    def test_to_dict_keeps_url_and_excerpt(self) -> None:
        # GIVEN a citation with every optional field set
        c = Citation(
            id="a",
            title="T",
            url="https://example.com",
            excerpt="…",
            source_type="web",
        )

        # WHEN it is serialised
        # THEN every field is carried through
        assert c.to_dict() == {
            "id": "a",
            "title": "T",
            "url": "https://example.com",
            "excerpt": "…",
            "source_type": "web",
        }

    def test_rejects_empty_id(self) -> None:
        # GIVEN an identifier made of whitespace only
        # WHEN the citation is built
        # THEN construction fails rather than producing an unreferenceable marker
        with pytest.raises(ValueError, match="non-empty"):
            Citation(id="  ", title="T")

    def test_rejects_forbidden_chars_in_id(self) -> None:
        # GIVEN an identifier containing a comma, which the markup uses
        # WHEN the citation is built
        # THEN construction fails rather than producing an ambiguous marker
        with pytest.raises(ValueError, match="may not contain"):
            Citation(id="a,b", title="T")

    def test_rejects_unknown_source_type(self) -> None:
        # GIVEN a source type outside the accepted set
        # WHEN the citation is built
        # THEN construction fails rather than storing an unknown kind
        with pytest.raises(ValueError, match="source_type"):
            Citation(id="a", title="T", source_type="podcast")  # type: ignore[arg-type]


class TestBuildCitationPayload:
    def test_deduplicates_by_id(self) -> None:
        # GIVEN three citations of which two share an identifier
        # WHEN the payload is built
        payload = build_citation_payload(
            [
                Citation(id="a", title="A"),
                Citation(id="a", title="A-dup"),
                Citation(id="b", title="B"),
            ]
        )

        # THEN the first occurrence wins and the duplicate is dropped
        assert [c["id"] for c in payload] == ["a", "b"]

    def test_empty_iterable_returns_empty(self) -> None:
        # GIVEN no citation at all
        # WHEN the payload is built
        # THEN the result is an empty list, not None
        assert build_citation_payload([]) == []

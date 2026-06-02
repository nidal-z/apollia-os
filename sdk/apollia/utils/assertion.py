"""Confidence and citation markup helpers.

Agents emit assertions with explicit confidence levels and citations by
wrapping spans of their output in ``[conf:<level>]...[/conf]`` markers and
dropping inline ``[cite:<id>]`` pointers. The runtime (``apollia-runtime``
``confidence_parser``) parses these markers server-side and the desktop UI
renders confidence badges and clickable citation footnotes.

Example:
-------

.. code-block:: python

    from apollia.utils.assertion import assert_with_confidence, Citation

    nasa = Citation(
        id="nasa-2024",
        title="NASA Earth Fact Sheet",
        url="https://nssdc.gsfc.nasa.gov/planetary/factsheet/earthfact.html",
        source_type="web",
    )

    text = assert_with_confidence(
        "Earth has one natural satellite",
        level="high",
        citations=[nasa],
    )
    # → "[conf:high]Earth has one natural satellite[cite:nasa-2024][/conf]"

The helper never emits LLM calls and never escapes its input - callers are
responsible for passing plain text (no raw ``[conf:…]`` markers).
"""

from __future__ import annotations

from collections.abc import Iterable
from dataclasses import asdict, dataclass, field
from typing import Literal

ConfidenceLevel = Literal["high", "medium", "low"]
SourceType = Literal["web", "document", "tool", "memory", "other"]

_VALID_LEVELS: frozenset[str] = frozenset({"high", "medium", "low"})
_VALID_SOURCE_TYPES: frozenset[str] = frozenset({"web", "document", "tool", "memory", "other"})


@dataclass(frozen=True)
class Citation:
    """A bibliographic entry referenced by one or more assertions.

    Attributes:
        id: Stable identifier referenced by ``[cite:<id>]`` markers.
        title: Human-readable title shown in the footnote.
        url: Optional URL rendered as a clickable link.
        excerpt: Optional excerpt surfaced in the tooltip / side-panel.
        source_type: Broad classification driving the badge icon.
    """

    id: str
    title: str
    url: str | None = None
    excerpt: str | None = None
    source_type: SourceType = "other"

    def __post_init__(self) -> None:
        if not self.id or not self.id.strip():
            raise ValueError("Citation.id must be a non-empty string")
        if "," in self.id or "]" in self.id or "[" in self.id:
            raise ValueError("Citation.id may not contain '[', ']' or ',' characters")
        if self.source_type not in _VALID_SOURCE_TYPES:
            raise ValueError(f"Citation.source_type must be one of {sorted(_VALID_SOURCE_TYPES)}")

    def to_dict(self) -> dict[str, object]:
        """Return a JSON-serialisable dict, dropping ``None`` fields."""
        return {k: v for k, v in asdict(self).items() if v is not None}


@dataclass(frozen=True)
class AssertionSpec:
    """Raw assertion spec - mirrors the Rust ``Assertion`` struct."""

    text: str
    level: ConfidenceLevel
    citations: tuple[Citation, ...] = field(default_factory=tuple)


def assert_with_confidence(
    text: str,
    level: ConfidenceLevel,
    citations: Iterable[Citation] | None = None,
) -> str:
    """Wrap *text* with confidence + citation markers.

    Args:
        text: Plain text (no existing ``[conf:…]`` markers).
        level: One of ``"high"``, ``"medium"``, ``"low"``.
        citations: Optional iterable of :class:`Citation` to attach. The
            citation objects themselves should also be surfaced via
            :func:`build_citation_payload` in the agent's ``AIPResult``
            metadata so the UI can render the footnote.

    Returns:
        The text wrapped in ``[conf:<level>]…[cite:…]…[/conf]`` markup,
        ready to be embedded verbatim in an agent's message part.

    Raises:
        ValueError: If *level* is not a valid confidence level, or *text*
            already contains ``[conf:…]`` / ``[/conf]`` markers.
    """
    if level not in _VALID_LEVELS:
        raise ValueError(f"level must be one of {sorted(_VALID_LEVELS)}, got {level!r}")
    if "[conf:" in text or "[/conf]" in text:
        raise ValueError("text must not already contain [conf:...] markers")

    cite_markers = "".join(f"[cite:{c.id}]" for c in (citations or ()))
    return f"[conf:{level}]{text}{cite_markers}[/conf]"


def build_citation_payload(citations: Iterable[Citation]) -> list[dict[str, object]]:
    """Serialise a list of citations for ``AIPResult.metadata['citations']``.

    The runtime reads this list and pairs each entry with the assertions
    produced by the inline markers. Returns a stable-ordered list of
    dictionaries, deduplicated by ``id``.
    """
    seen: set[str] = set()
    out: list[dict[str, object]] = []
    for c in citations:
        if c.id in seen:
            continue
        seen.add(c.id)
        out.append(c.to_dict())
    return out


__all__ = [
    "AssertionSpec",
    "Citation",
    "ConfidenceLevel",
    "SourceType",
    "assert_with_confidence",
    "build_citation_payload",
]

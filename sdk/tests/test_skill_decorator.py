"""Tests for ``@skill`` decorator."""

from __future__ import annotations

import pytest
from apollia._internal.manifest import SKILL_ATTR
from apollia.errors import AgentConfigError
from apollia.skills import skill

# ──────────────────────────────────────────────────────────────────────
# Marker placement
# ──────────────────────────────────────────────────────────────────────


def test_skill_marker_set_with_defaults() -> None:
    # GIVEN a handler decorated with a skill id and nothing else
    # WHEN the marker attribute is read
    # THEN every optional field carries its documented default
    @skill("pdf.read")
    async def fn(self: object) -> dict[str, str]:
        return {"ok": "true"}

    meta = getattr(fn, SKILL_ATTR)
    assert meta == {
        "id": "pdf.read",
        "description": "",
        "dangerous": False,
        "examples": [],
    }


def test_skill_marker_with_all_args() -> None:
    # GIVEN a handler decorated with a description and dangerous=True
    # WHEN the marker attribute is read
    # THEN both are carried, and no legacy requires_approval key is invented
    @skill(
        "billing.charge",
        description="Charge a customer",
        dangerous=True,
    )
    async def fn(self: object) -> dict[str, str]:
        return {"ok": "true"}

    meta = getattr(fn, SKILL_ATTR)
    assert meta["id"] == "billing.charge"
    assert meta["description"] == "Charge a customer"
    assert "requires_approval" not in meta
    assert meta["dangerous"] is True


def test_skill_returns_method_unchanged() -> None:
    # GIVEN an undecorated async handler
    # WHEN the decorator is applied to it
    # THEN the same function object comes back, so the decorator is a pure marker
    async def original(self: object) -> dict[str, str]:  # NOSONAR
        return {"ok": "true"}

    decorated = skill("x.y")(original)
    assert decorated is original


# ──────────────────────────────────────────────────────────────────────
# skill_id validation
# ──────────────────────────────────────────────────────────────────────


def test_skill_id_empty_raises() -> None:
    # GIVEN an empty skill id
    # WHEN a handler is decorated with it
    # THEN it is refused at decoration time
    with pytest.raises(AgentConfigError, match="non-empty"):

        @skill("")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_uppercase_raises() -> None:
    # GIVEN a skill id carrying uppercase letters
    # WHEN a handler is decorated with it
    # THEN it is refused rather than lowercased silently
    with pytest.raises(AgentConfigError, match="invalid"):

        @skill("Pdf.Read")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_with_spaces_raises() -> None:
    # GIVEN a skill id containing a space
    # WHEN a handler is decorated with it
    # THEN it is refused, because a skill id is one token
    with pytest.raises(AgentConfigError, match="invalid"):

        @skill("pdf read")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_with_slash_raises() -> None:
    # GIVEN a skill id using a slash as separator
    # WHEN a handler is decorated with it
    # THEN it is refused, because the separator is the dot
    with pytest.raises(AgentConfigError, match="invalid"):

        @skill("pdf/read")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_leading_digit_raises() -> None:
    # GIVEN a skill id whose first segment starts with a digit
    # WHEN a handler is decorated with it
    # THEN it is refused, because a segment is an identifier
    with pytest.raises(AgentConfigError, match="invalid"):

        @skill("1pdf.read")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_trailing_dot_raises() -> None:
    # GIVEN a skill id ending on a separator
    # WHEN a handler is decorated with it
    # THEN it is refused rather than yielding an empty last segment
    with pytest.raises(AgentConfigError, match="invalid"):

        @skill("pdf.")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_double_dot_raises() -> None:
    # GIVEN a skill id with two consecutive separators
    # WHEN a handler is decorated with it
    # THEN it is refused rather than yielding an empty middle segment
    with pytest.raises(AgentConfigError, match="invalid"):

        @skill("pdf..read")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_simple_valid() -> None:
    # GIVEN a skill id with a single segment
    # WHEN the marker attribute is read
    # THEN it is accepted, so a namespace is not mandatory
    @skill("read")
    async def fn(self: object) -> dict[str, str]:
        return {}

    assert getattr(fn, SKILL_ATTR)["id"] == "read"


def test_skill_id_deep_namespace_valid() -> None:
    # GIVEN a skill id with four segments
    # WHEN the marker attribute is read
    # THEN it is accepted, so nesting is not capped at two levels
    @skill("a.b.c.d")
    async def fn(self: object) -> dict[str, str]:
        return {}

    assert getattr(fn, SKILL_ATTR)["id"] == "a.b.c.d"


def test_skill_id_with_digits_and_underscore_valid() -> None:
    # GIVEN a skill id with digits and underscores inside its segments
    # WHEN the marker attribute is read
    # THEN it is accepted, so only the leading character is constrained
    @skill("pdf.read_text_v2")
    async def fn(self: object) -> dict[str, str]:
        return {}

    assert getattr(fn, SKILL_ATTR)["id"] == "pdf.read_text_v2"


def test_skill_id_none_raises() -> None:
    # GIVEN None passed where a skill id is expected
    # WHEN the decorator factory is called
    # THEN it refuses instead of stringifying None into an id
    with pytest.raises(AgentConfigError):
        skill(None)  # type: ignore[arg-type]  # NOSONAR


# ──────────────────────────────────────────────────────────────────────
# async-only enforcement
# ──────────────────────────────────────────────────────────────────────


def test_skill_on_sync_method_raises() -> None:
    # GIVEN a handler declared with def, not async def
    # WHEN it is decorated as a skill
    # THEN it is refused at decoration time rather than at dispatch time
    with pytest.raises(AgentConfigError, match="async def"):

        @skill("pdf.read")
        def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_on_async_method_ok() -> None:
    # GIVEN a handler declared with async def
    # WHEN the marker attribute is read
    # THEN the skill id is there, so the async form is the accepted one
    @skill("pdf.read")
    async def fn(self: object) -> dict[str, str]:
        return {}

    assert getattr(fn, SKILL_ATTR)["id"] == "pdf.read"


# ──────────────────────────────────────────────────────────────────────
# Double application detection
# ──────────────────────────────────────────────────────────────────────


def test_skill_applied_twice_raises() -> None:
    # GIVEN a handler already decorated once
    # WHEN the decorator is applied to it a second time
    # THEN it is refused, because one handler carries one skill
    async def fn(self: object) -> dict[str, str]:  # NOSONAR
        return {}

    skill("a.b")(fn)
    with pytest.raises(AgentConfigError, match="already applied"):
        skill("a.b")(fn)


def test_skill_description_must_be_string() -> None:
    # GIVEN a description that is not a string
    # WHEN a handler is decorated with it
    # THEN it is refused instead of being coerced
    with pytest.raises(AgentConfigError, match="description"):

        @skill("x.y", description=123)  # type: ignore[arg-type]  # NOSONAR
        async def fn(self: object) -> dict[str, str]:
            return {}


# Tests for the examples= decorator keyword


def test_skill_examples_propagated() -> None:
    """``examples=`` is normalized and stamped on the SKILL_ATTR marker."""
    # GIVEN a skill decorated with two payload examples
    # WHEN the marker attribute is read
    # THEN both examples are carried, in order and with their structure intact

    @skill(
        "chart.bar",
        description="Generate a bar chart",
        examples=[
            {"series": [{"name": "Q1", "values": [1, 2, 3]}]},
            {"series": [{"name": "Sales", "values": [10]}], "title": "2026"},
        ],
    )
    async def fn(self: object) -> dict[str, str]:
        return {}

    meta = getattr(fn, SKILL_ATTR)
    assert len(meta["examples"]) == 2
    assert meta["examples"][0]["series"][0]["name"] == "Q1"
    assert meta["examples"][1]["title"] == "2026"


def test_skill_examples_default_empty_list() -> None:
    """When no ``examples=`` provided, the marker carries an empty list."""
    # GIVEN a skill decorated without examples
    # WHEN the marker attribute is read
    # THEN the examples field is an empty list, not None

    @skill("x.y")
    async def fn(self: object) -> dict[str, str]:
        return {}

    assert getattr(fn, SKILL_ATTR)["examples"] == []


def test_skill_examples_must_be_list() -> None:
    # GIVEN examples passed as a mapping rather than a list
    # WHEN a handler is decorated with them
    # THEN they are refused instead of being wrapped
    with pytest.raises(AgentConfigError, match="examples"):

        @skill("x.y", examples={"not": "a list"})  # type: ignore[arg-type]
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_examples_must_be_list_of_dicts() -> None:
    # GIVEN a list of examples holding one entry that is not a mapping
    # WHEN a handler is decorated with them
    # THEN the whole list is refused, so a bad entry cannot slip through
    with pytest.raises(AgentConfigError, match="examples"):

        @skill("x.y", examples=[{"ok": 1}, "not a dict"])  # type: ignore[list-item]
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_examples_isolated_from_caller_mutation() -> None:
    """Mutating the caller's list after decoration must not affect the marker."""
    # GIVEN a skill decorated with a list the caller still holds
    # WHEN the caller appends to that list after decoration
    # THEN the marker is unchanged, so the decorator copied the list

    samples = [{"a": 1}]

    @skill("x.y", examples=samples)
    async def fn(self: object) -> dict[str, str]:
        return {}

    samples.append({"b": 2})
    assert len(getattr(fn, SKILL_ATTR)["examples"]) == 1

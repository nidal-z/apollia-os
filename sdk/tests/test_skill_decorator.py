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
    @skill("pdf.read")
    async def fn(self: object) -> dict[str, str]:
        return {"ok": "true"}

    meta = getattr(fn, SKILL_ATTR)
    assert meta == {
        "id": "pdf.read",
        "description": "",
        "requires_approval": False,
        "dangerous": False,
        "examples": [],
    }


def test_skill_marker_with_all_args() -> None:
    @skill(
        "billing.charge",
        description="Charge a customer",
        requires_approval=True,
        dangerous=True,
    )
    async def fn(self: object) -> dict[str, str]:
        return {"ok": "true"}

    meta = getattr(fn, SKILL_ATTR)
    assert meta["id"] == "billing.charge"
    assert meta["description"] == "Charge a customer"
    assert meta["requires_approval"] is True
    assert meta["dangerous"] is True


def test_skill_returns_method_unchanged() -> None:
    async def original(self: object) -> dict[str, str]:  # NOSONAR
        return {"ok": "true"}

    decorated = skill("x.y")(original)
    assert decorated is original


# ──────────────────────────────────────────────────────────────────────
# skill_id validation
# ──────────────────────────────────────────────────────────────────────


def test_skill_id_empty_raises() -> None:
    with pytest.raises(AgentConfigError, match="non-empty"):

        @skill("")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_uppercase_raises() -> None:
    with pytest.raises(AgentConfigError, match="invalid"):

        @skill("Pdf.Read")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_with_spaces_raises() -> None:
    with pytest.raises(AgentConfigError, match="invalid"):

        @skill("pdf read")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_with_slash_raises() -> None:
    with pytest.raises(AgentConfigError, match="invalid"):

        @skill("pdf/read")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_leading_digit_raises() -> None:
    with pytest.raises(AgentConfigError, match="invalid"):

        @skill("1pdf.read")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_trailing_dot_raises() -> None:
    with pytest.raises(AgentConfigError, match="invalid"):

        @skill("pdf.")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_double_dot_raises() -> None:
    with pytest.raises(AgentConfigError, match="invalid"):

        @skill("pdf..read")
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_id_simple_valid() -> None:
    @skill("read")
    async def fn(self: object) -> dict[str, str]:
        return {}

    assert getattr(fn, SKILL_ATTR)["id"] == "read"


def test_skill_id_deep_namespace_valid() -> None:
    @skill("a.b.c.d")
    async def fn(self: object) -> dict[str, str]:
        return {}

    assert getattr(fn, SKILL_ATTR)["id"] == "a.b.c.d"


def test_skill_id_with_digits_and_underscore_valid() -> None:
    @skill("pdf.read_text_v2")
    async def fn(self: object) -> dict[str, str]:
        return {}

    assert getattr(fn, SKILL_ATTR)["id"] == "pdf.read_text_v2"


def test_skill_id_none_raises() -> None:
    with pytest.raises(AgentConfigError):
        skill(None)  # type: ignore[arg-type]  # NOSONAR


# ──────────────────────────────────────────────────────────────────────
# async-only enforcement
# ──────────────────────────────────────────────────────────────────────


def test_skill_on_sync_method_raises() -> None:
    with pytest.raises(AgentConfigError, match="async def"):

        @skill("pdf.read")
        def fn(self: object) -> dict[str, str]:  # noqa: E306
            return {}


def test_skill_on_async_method_ok() -> None:
    @skill("pdf.read")
    async def fn(self: object) -> dict[str, str]:
        return {}

    assert getattr(fn, SKILL_ATTR)["id"] == "pdf.read"


# ──────────────────────────────────────────────────────────────────────
# Double application detection
# ──────────────────────────────────────────────────────────────────────


def test_skill_applied_twice_raises() -> None:
    async def fn(self: object) -> dict[str, str]:  # NOSONAR
        return {}

    skill("a.b")(fn)
    with pytest.raises(AgentConfigError, match="already applied"):
        skill("a.b")(fn)


def test_skill_description_must_be_string() -> None:
    with pytest.raises(AgentConfigError, match="description"):

        @skill("x.y", description=123)  # type: ignore[arg-type]  # NOSONAR
        async def fn(self: object) -> dict[str, str]:
            return {}


# ──────────────────────────────────────────────────────────────────────  # NOSONAR
# examples= keyword
# ──────────────────────────────────────────────────────────────────────


def test_skill_examples_propagated() -> None:
    """``examples=`` is normalized and stamped on the SKILL_ATTR marker."""

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

    @skill("x.y")
    async def fn(self: object) -> dict[str, str]:
        return {}

    assert getattr(fn, SKILL_ATTR)["examples"] == []


def test_skill_examples_must_be_list() -> None:
    with pytest.raises(AgentConfigError, match="examples"):

        @skill("x.y", examples={"not": "a list"})  # type: ignore[arg-type]
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_examples_must_be_list_of_dicts() -> None:
    with pytest.raises(AgentConfigError, match="examples"):

        @skill("x.y", examples=[{"ok": 1}, "not a dict"])  # type: ignore[list-item]
        async def fn(self: object) -> dict[str, str]:
            return {}


def test_skill_examples_isolated_from_caller_mutation() -> None:
    """Mutating the caller's list after decoration must not affect the marker."""

    samples = [{"a": 1}]

    @skill("x.y", examples=samples)
    async def fn(self: object) -> dict[str, str]:
        return {}

    samples.append({"b": 2})
    assert len(getattr(fn, SKILL_ATTR)["examples"]) == 1

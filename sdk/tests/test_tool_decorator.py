"""Tests for the ``@tool`` decorator."""

from typing import Annotated, Any, NotRequired, TypedDict

import pytest
from apollia import tool
from apollia.errors import AgentConfigError
from apollia.tool_decorator import ACTION_FAMILIES, TOOL_ATTR, ToolSpec


class Invoice(TypedDict):
    path: Annotated[str, "path of the invoice"]
    currency: NotRequired[str]


def test_tool_infers_its_schema_from_typeddict_and_annotated() -> None:
    # GIVEN a tool whose parameters mix a TypedDict, an Annotated scalar and a default
    @tool(
        "extract_totals",
        "Read the totals of an invoice.",
        action="read",
        examples=[{"invoice": {"path": "a.pdf"}, "rounding": 2}],
    )
    async def extract_totals(
        ctx: Any, invoice: Invoice, rounding: Annotated[int, "decimal places"] = 2
    ) -> dict[str, float]:
        return {}

    # WHEN the stamped spec is read
    spec = getattr(extract_totals, TOOL_ATTR)

    # THEN the schema is inferred from the signature, ctx left out
    assert isinstance(spec, ToolSpec)
    assert spec.input_schema == {
        "type": "object",
        "properties": {
            "invoice": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "path of the invoice"},
                    "currency": {"type": "string"},
                },
                "required": ["path"],
                "additionalProperties": False,
            },
            "rounding": {"type": "integer", "description": "decimal places"},
        },
        "required": ["invoice"],
        "additionalProperties": False,
    }
    assert spec.examples == [{"invoice": {"path": "a.pdf"}, "rounding": 2}]


def test_tool_carries_its_family_and_derives_sensitivity_from_it() -> None:
    # GIVEN one tool per action family
    specs: dict[str, ToolSpec] = {}
    for family in ACTION_FAMILIES:

        @tool(f"tool_{family}", "Does one thing.", action=family)
        async def fn(ctx: Any) -> None:
            return None

        specs[family] = getattr(fn, TOOL_ATTR)

    # WHEN the families and sensitivity are read
    # THEN each spec carries its family, and only `read` is not sensitive
    assert {f: s.action for f, s in specs.items()} == {f: f for f in ACTION_FAMILIES}
    assert [f for f, s in specs.items() if not s.sensitive] == ["read"]


def test_tool_refuses_a_function_that_is_not_async() -> None:
    # GIVEN a plain def function
    decorate = tool("sync_tool", "Not async.", action="read")

    # WHEN it is decorated
    # THEN the declaration is refused at import time
    with pytest.raises(AgentConfigError, match="must be 'async def'"):

        @decorate
        def sync_tool(ctx: Any) -> None:
            return None


@pytest.mark.parametrize("family", ["delete", "write", "send", "READ", ""])
def test_tool_refuses_an_unknown_family(family: str) -> None:
    # GIVEN a family outside the five apollia.toml accepts
    # WHEN a tool declares it
    # THEN the declaration is refused and the known families are named
    with pytest.raises(AgentConfigError, match="not a known family; expected one of read"):
        tool("bad_family", "Unknown family.", action=family)


def test_tool_takes_no_dangerous_flag() -> None:
    # GIVEN a tool declared with the retired dangerous flag
    # WHEN the decorator is called
    # THEN Python refuses the unknown keyword, sensitivity following the family
    with pytest.raises(TypeError, match="dangerous"):
        tool("purge", "Delete old rows.", action="write_or_delete", dangerous=True)  # type: ignore[call-arg]


def test_tool_sensitivity_follows_the_family() -> None:
    # GIVEN a read tool and a write tool
    @tool("peek", "Read rows.", action="read")
    async def peek(ctx: Any) -> None:
        return None

    @tool("purge", "Delete old rows.", action="write_or_delete")
    async def purge(ctx: Any) -> None:
        return None

    # WHEN their specs are read
    peek_spec = getattr(peek, TOOL_ATTR)
    purge_spec = getattr(purge, TOOL_ATTR)

    # THEN only the non-read family is sensitive, and no flag is recorded
    assert peek_spec.sensitive is False
    assert purge_spec.sensitive is True
    assert not hasattr(purge_spec, "dangerous")


@pytest.mark.parametrize("name", ["Extract", "has space", "dot.ted", "", 3])
def test_tool_refuses_a_name_outside_the_manifest_alphabet(name: object) -> None:
    # GIVEN a name apollia.toml would refuse
    # WHEN a tool declares it
    # THEN the declaration is refused
    with pytest.raises(AgentConfigError, match=r"must match \[a-z0-9_-\]\+"):
        tool(name, "Bad name.", action="read")  # type: ignore[arg-type]


def test_tool_refuses_an_empty_description_and_bad_examples() -> None:
    # GIVEN a blank description, then examples that are not dicts
    # WHEN each is declared
    # THEN each is refused
    with pytest.raises(AgentConfigError, match="description must be a non-empty string"):
        tool("blank", "  ", action="read")
    with pytest.raises(AgentConfigError, match=r"examples\[0\] must be a dict"):
        tool("bad_examples", "Bad.", action="read", examples=["x"])  # type: ignore[list-item]


def test_tool_refuses_being_applied_twice() -> None:
    # GIVEN a function already declared as a tool
    @tool("once", "Once.", action="read")
    async def once(ctx: Any) -> None:
        return None

    # WHEN it is declared a second time
    # THEN the second declaration is refused
    with pytest.raises(AgentConfigError, match="already applied"):
        tool("twice", "Twice.", action="read")(once)

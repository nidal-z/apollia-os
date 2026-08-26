"""Tests for the manifest builder."""

from __future__ import annotations

from typing import Any

import pytest
from apollia._internal.manifest import (
    ON_MESSAGE_ATTR,
    ON_MESSAGE_HANDLER_ATTR,
    ORCHESTRATED_ATTR,
    SKILL_ATTR,
    SKILLS_REGISTRY_ATTR,
    build_manifest,
    collect_skills,
    find_on_message_handler,
    find_orchestrated_config,
)
from apollia.errors import AgentConfigError


def _mark_skill(
    *,
    skill_id: str | None = None,
    description: str = "",
    dangerous: bool = False,
    examples: list[dict[str, Any]] | None = None,
) -> Any:
    """Test-only stand-in for the future ``@skill`` decorator."""

    def wrap(fn: Any) -> Any:
        setattr(
            fn,
            SKILL_ATTR,
            {
                "id": skill_id,
                "description": description,
                "dangerous": dangerous,
                "examples": list(examples) if examples else [],
            },
        )
        return fn

    return wrap


def _mark_on_message(fn: Any) -> Any:
    setattr(fn, ON_MESSAGE_ATTR, True)
    return fn


# ──────────────────────────── direct mode ────────────────────────────


def test_build_manifest_direct_skills() -> None:
    # GIVEN a class carrying two marked skills
    class Agent:
        @_mark_skill(skill_id="parse.pdf", description="Parse a PDF")
        def parse_pdf(self, path: str) -> str:
            return path

        @_mark_skill(skill_id="parse.docx", description="Parse a DOCX")
        def parse_docx(self, path: str) -> str:
            return path

    # WHEN the manifest is built from it
    manifest = build_manifest(
        Agent,
        name="parser",
        version="0.1.0",
        description="parses files",
    )
    # THEN the mode is direct, a2a is on, and each skill carries a schema of its arguments
    assert manifest["name"] == "parser"
    assert manifest["version"] == "0.1.0"
    assert manifest["supports_a2a"] is True
    assert manifest["execution_mode"] == "direct"
    skill_ids = {s["id"] for s in manifest["skills"]}
    assert skill_ids == {"parse.pdf", "parse.docx"}
    for skill in manifest["skills"]:
        assert skill["input_schema"]["type"] == "object"
        assert "path" in skill["input_schema"]["properties"]


def test_build_manifest_propagates_lists() -> None:
    # GIVEN a class with no handler and every declaration list filled
    class Agent:
        pass

    # WHEN the manifest is built from it
    manifest = build_manifest(
        Agent,
        name="x",
        version="1.0.0",
        description="desc",
        datasources=("ds1", "ds2"),
        templates=("t1",),
        secrets=("S1",),
        tools_required=("bash_executor",),
        tags=("alpha",),
        packages=("pkg",),
        shared_memory_namespaces=("ns1",),
    )
    # THEN every tuple becomes a list, a2a is off, and no skill is invented
    assert manifest["datasources"] == ["ds1", "ds2"]
    assert manifest["templates"] == ["t1"]
    assert manifest["secrets"] == ["S1"]
    assert manifest["tools_required"] == ["bash_executor"]
    assert manifest["tags"] == ["alpha"]
    assert manifest["packages"] == ["pkg"]
    assert manifest["shared_memory_namespaces"] == ["ns1"]
    assert manifest["supports_a2a"] is False
    assert manifest["execution_mode"] == "direct"
    assert manifest["skills"] == []


def test_build_manifest_step_budget_optional() -> None:
    # GIVEN a class declared without budget, type or memory namespace
    class Agent:
        pass

    # WHEN the manifest is built from it
    manifest = build_manifest(Agent, name="x", version="1.0.0", description="d")
    # THEN those three fields stay None rather than taking an invented default
    assert manifest["step_budget"] is None
    assert manifest["agent_type"] is None
    assert manifest["memory_namespace"] is None


def test_build_manifest_max_concurrent_default_is_one() -> None:
    # GIVEN a class declared without a concurrency limit
    class Agent:
        pass

    # WHEN the manifest is built from it
    manifest = build_manifest(Agent, name="x", version="1.0.0", description="d")
    # THEN the limit defaults to one, so an agent is serial unless it says otherwise
    assert manifest["max_concurrent_tasks"] == 1


def test_build_manifest_max_concurrent_set() -> None:
    # GIVEN a class declaring a concurrency limit of three
    class Agent:
        pass

    # WHEN the manifest is built from it
    manifest = build_manifest(
        Agent, name="x", version="1.0.0", description="d", max_concurrent_tasks=3
    )
    # THEN the declared value is carried through
    assert manifest["max_concurrent_tasks"] == 3


def test_build_manifest_max_concurrent_rejects_below_one() -> None:
    # GIVEN a class declaring a concurrency limit of zero
    class Agent:
        pass

    # WHEN the manifest is built from it
    # THEN it is refused rather than silently clamped
    with pytest.raises(AgentConfigError):
        build_manifest(Agent, name="x", version="1.0.0", description="d", max_concurrent_tasks=0)


def test_build_manifest_max_concurrent_rejects_non_int() -> None:
    # GIVEN a class declaring True as its concurrency limit
    class Agent:
        pass

    # WHEN the manifest is built from it
    # bool is an int subclass but must be rejected.
    # THEN it is refused, even though bool is a subclass of int
    with pytest.raises(AgentConfigError):
        build_manifest(
            Agent,
            name="x",
            version="1.0.0",
            description="d",
            max_concurrent_tasks=True,  # type: ignore[arg-type]
        )


# ──────────────────────────── conversational mode ────────────────────────────


def test_build_manifest_conversational() -> None:
    # GIVEN a class carrying one marked message handler
    class Agent:
        @_mark_on_message
        def handle(self, message: str, history: list[dict[str, Any]], ctx: Any) -> str:
            return message

    # WHEN the manifest is built from it
    manifest = build_manifest(Agent, name="chat", version="1.0.0", description="d")
    # THEN the mode is conversational, a2a is off, and the handler name is stamped on the class
    assert manifest["execution_mode"] == "conversational"
    assert manifest["supports_a2a"] is False
    assert getattr(Agent, ON_MESSAGE_HANDLER_ATTR) == "handle"


# ──────────────────────────── orchestrated mode ────────────────────────────


def test_build_manifest_orchestrated() -> None:
    # GIVEN a class carrying an orchestration config and no handler
    class Agent:
        pass

    setattr(Agent, ORCHESTRATED_ATTR, {"system_prompt": "be helpful"})

    # WHEN the manifest is built from it
    manifest = build_manifest(Agent, name="oria", version="1.0.0", description="d")
    # THEN the mode is orchestrated and the system prompt is carried
    assert manifest["execution_mode"] == "orchestrated"
    assert manifest["system_prompt"] == "be helpful"


def test_orchestrated_with_skill_raises() -> None:
    # GIVEN a class carrying both an orchestration config and a skill
    class Agent:
        @_mark_skill(skill_id="a")
        def a(self, x: int) -> str:
            return str(x)

    setattr(Agent, ORCHESTRATED_ATTR, {"system_prompt": "x"})

    # WHEN the manifest is built from it
    # THEN it is refused, because the two execution modes are exclusive
    with pytest.raises(AgentConfigError):
        build_manifest(Agent, name="x", version="1.0.0", description="d")


def test_orchestrated_with_on_message_raises() -> None:
    # GIVEN a class carrying both an orchestration config and a message handler
    class Agent:
        @_mark_on_message
        def handle(self, message: str) -> str:
            return message

    setattr(Agent, ORCHESTRATED_ATTR, {"system_prompt": "x"})

    # WHEN the manifest is built from it
    # THEN it is refused, because the two execution modes are exclusive
    with pytest.raises(AgentConfigError):
        build_manifest(Agent, name="x", version="1.0.0", description="d")


# ──────────────────────────── validation ────────────────────────────


def test_empty_name_raises() -> None:
    # GIVEN a class declared with an empty name
    class Agent:
        pass

    # WHEN the manifest is built from it
    # THEN it is refused at build time rather than registered nameless
    with pytest.raises(AgentConfigError):
        build_manifest(Agent, name="", version="1.0.0", description="d")


def test_empty_version_raises() -> None:
    # GIVEN a class declared with an empty version
    class Agent:
        pass

    # WHEN the manifest is built from it
    # THEN it is refused at build time
    with pytest.raises(AgentConfigError):
        build_manifest(Agent, name="x", version="", description="d")


def test_invalid_datasource_raises() -> None:
    # GIVEN a class declaring an empty string as a datasource name
    class Agent:
        pass

    # WHEN the manifest is built from it
    # THEN it is refused rather than carried into the manifest
    with pytest.raises(AgentConfigError):
        build_manifest(
            Agent,
            name="x",
            version="1.0.0",
            description="d",
            datasources=("",),  # empty string rejected
        )


def test_duplicate_skill_ids_raise() -> None:
    # GIVEN a class whose two methods claim the same skill id
    class Agent:
        @_mark_skill(skill_id="dup")
        def a(self, x: int) -> str:
            return str(x)

        @_mark_skill(skill_id="dup")
        def b(self, x: int) -> str:
            return str(x)

    # WHEN the manifest is built from it
    # THEN it is refused, because dispatch would be ambiguous
    with pytest.raises(AgentConfigError):
        build_manifest(Agent, name="x", version="1.0.0", description="d")


def test_multiple_on_message_raises() -> None:
    # GIVEN a class carrying two marked message handlers
    class Agent:
        @_mark_on_message
        def a(self, message: str) -> str:
            return message

        @_mark_on_message
        def b(self, message: str) -> str:
            return message

    # WHEN the handler is looked up
    # THEN it is refused, because a class carries at most one
    with pytest.raises(AgentConfigError):
        find_on_message_handler(Agent)


def test_collect_skills_no_skills() -> None:
    # GIVEN a class with a plain method and no marker
    # WHEN its skills are collected
    # THEN the registry is empty rather than holding the plain method
    class Agent:
        def regular(self) -> None: ...

    assert collect_skills(Agent) == {}


def test_find_on_message_none() -> None:
    # GIVEN a class with a plain method and no marker
    # WHEN the message handler is looked up
    # THEN None comes back rather than the plain method
    class Agent:
        def regular(self) -> None: ...

    assert find_on_message_handler(Agent) is None


def test_find_orchestrated_none() -> None:
    # GIVEN a class with no orchestration config
    # WHEN the config is looked up
    # THEN None comes back
    class Agent:
        pass

    assert find_orchestrated_config(Agent) is None


def test_find_orchestrated_invalid_type_raises() -> None:
    # GIVEN a class whose orchestration config is a string, not a mapping
    class Agent:
        pass

    setattr(Agent, ORCHESTRATED_ATTR, "not a dict")

    # WHEN the config is looked up
    # THEN it is refused rather than read as a mapping
    with pytest.raises(AgentConfigError):
        find_orchestrated_config(Agent)


def test_skill_id_defaults_to_method_name() -> None:
    # GIVEN a marked skill declaring no id
    class Agent:
        @_mark_skill(skill_id=None, description="default id")
        def parse_pdf(self, path: str) -> str:
            return path

    # WHEN its skills are collected
    registry = collect_skills(Agent)
    # THEN the method name becomes the skill id
    assert "parse_pdf" in registry


def test_registry_cached_on_class() -> None:
    # GIVEN a class carrying one marked skill
    class Agent:
        @_mark_skill(skill_id="x")
        def x(self, a: int) -> str:
            return str(a)

    # WHEN the manifest is built from it
    build_manifest(Agent, name="a", version="1.0.0", description="d")
    # THEN the collected registry is cached on the class for dispatch to reuse
    cached = getattr(Agent, SKILLS_REGISTRY_ATTR)
    assert "x" in cached


# ──────────────────────────── docstring fallback ────────────────────────────


def test_collect_skills_falls_back_to_docstring() -> None:
    """An empty ``description=`` falls back to the first line of the docstring."""
    # GIVEN a marked skill with an empty description and a docstring

    class Agent:
        @_mark_skill(skill_id="chart.bar", description="")
        def bar(self, x: int) -> str:
            """Generate a bar chart from data series.

            Each series produces one colored bar group.
            """
            return str(x)

    # WHEN its skills are collected
    registry = collect_skills(Agent)
    # THEN the first line of the docstring becomes the description
    assert registry["chart.bar"].description == "Generate a bar chart from data series."


def test_collect_skills_docstring_first_paragraph_keeps_only_one_line() -> None:
    """First paragraph spanning multiple lines uses only the first non-empty line."""
    # GIVEN a marked skill whose docstring first paragraph spans two lines

    class Agent:
        @_mark_skill(skill_id="parse.pdf")
        def parse(self, path: str) -> str:
            """Parse a PDF file.
            Continued explanation that should not appear in the manifest.

            Second paragraph ignored.
            """
            return path

    # WHEN its skills are collected
    registry = collect_skills(Agent)
    # THEN only the first non-blank line becomes the description
    # ``inspect.getdoc`` strips leading indentation; first non-blank line wins.
    assert registry["parse.pdf"].description == "Parse a PDF file."


def test_collect_skills_explicit_description_wins_over_docstring() -> None:
    """An explicit ``description=`` overrides the docstring fallback."""
    # GIVEN a marked skill with both an explicit description and a docstring

    class Agent:
        @_mark_skill(skill_id="x", description="explicit wins")
        def x(self, a: int) -> str:
            """Docstring would normally be used."""
            return str(a)

    # WHEN its skills are collected
    registry = collect_skills(Agent)
    # THEN the explicit description wins over the fallback
    assert registry["x"].description == "explicit wins"


def test_collect_skills_no_docstring_no_description() -> None:
    """A skill with neither description nor docstring yields ``""``."""
    # GIVEN a marked skill with neither description nor docstring

    class Agent:
        @_mark_skill(skill_id="bare")
        def bare(self, a: int) -> str:
            return str(a)

    # WHEN its skills are collected
    registry = collect_skills(Agent)
    # THEN the description is the empty string, not None
    assert registry["bare"].description == ""


# ──────────────────────────── examples in manifest ────────────────────────────


def test_collect_skills_examples_in_manifest() -> None:
    """``examples=`` from the decorator is propagated into the skill manifest."""
    # GIVEN a marked skill carrying two payload examples
    samples: list[dict[str, Any]] = [{"a": 1}, {"a": 2, "b": "x"}]

    class Agent:
        @_mark_skill(skill_id="echo", examples=samples)
        def echo(self, a: int, b: str = "") -> dict[str, Any]:
            return {"a": a, "b": b}

    # WHEN the manifest is built from it
    manifest = build_manifest(Agent, name="x", version="1.0.0", description="d")
    # THEN the examples reach the skill entry unchanged
    skill_entry = next(s for s in manifest["skills"] if s["id"] == "echo")
    assert skill_entry["examples"] == samples


def test_collect_skills_no_examples_no_key_in_manifest() -> None:
    """When no examples are provided, the manifest entry omits ``examples``."""
    # GIVEN a marked skill carrying no example

    class Agent:
        @_mark_skill(skill_id="echo")
        def echo(self, a: int) -> dict[str, Any]:
            return {"a": a}

    # WHEN the manifest is built from it
    manifest = build_manifest(Agent, name="x", version="1.0.0", description="d")
    # THEN the examples key is absent rather than present and empty
    skill_entry = next(s for s in manifest["skills"] if s["id"] == "echo")
    assert "examples" not in skill_entry

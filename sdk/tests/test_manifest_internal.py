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
    requires_approval: bool = False,
    dangerous: bool = False,
) -> Any:
    """Test-only stand-in for the future ``@skill`` decorator."""

    def wrap(fn: Any) -> Any:
        setattr(
            fn,
            SKILL_ATTR,
            {
                "id": skill_id,
                "description": description,
                "requires_approval": requires_approval,
                "dangerous": dangerous,
            },
        )
        return fn

    return wrap


def _mark_on_message(fn: Any) -> Any:
    setattr(fn, ON_MESSAGE_ATTR, True)
    return fn


# ──────────────────────────── direct mode ────────────────────────────


def test_build_manifest_direct_skills() -> None:
    class Agent:
        @_mark_skill(skill_id="parse.pdf", description="Parse a PDF")
        def parse_pdf(self, path: str) -> str:
            return path

        @_mark_skill(skill_id="parse.docx", description="Parse a DOCX")
        def parse_docx(self, path: str) -> str:
            return path

    manifest = build_manifest(
        Agent,
        name="parser",
        version="0.1.0",
        description="parses files",
    )
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
    class Agent:
        pass

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
    class Agent:
        pass

    manifest = build_manifest(Agent, name="x", version="1.0.0", description="d")
    assert manifest["step_budget"] is None
    assert manifest["agent_type"] is None
    assert manifest["memory_namespace"] is None


# ──────────────────────────── conversational mode ────────────────────────────


def test_build_manifest_conversational() -> None:
    class Agent:
        @_mark_on_message
        def handle(self, message: str, history: list[dict[str, Any]], ctx: Any) -> str:
            return message

    manifest = build_manifest(Agent, name="chat", version="1.0.0", description="d")
    assert manifest["execution_mode"] == "conversational"
    assert manifest["supports_a2a"] is False
    assert getattr(Agent, ON_MESSAGE_HANDLER_ATTR) == "handle"


# ──────────────────────────── orchestrated mode ────────────────────────────


def test_build_manifest_orchestrated() -> None:
    class Agent:
        pass

    setattr(Agent, ORCHESTRATED_ATTR, {"system_prompt": "be helpful"})

    manifest = build_manifest(Agent, name="oria", version="1.0.0", description="d")
    assert manifest["execution_mode"] == "orchestrated"
    assert manifest["system_prompt"] == "be helpful"


def test_orchestrated_with_skill_raises() -> None:
    class Agent:
        @_mark_skill(skill_id="a")
        def a(self, x: int) -> str:
            return str(x)

    setattr(Agent, ORCHESTRATED_ATTR, {"system_prompt": "x"})

    with pytest.raises(AgentConfigError):
        build_manifest(Agent, name="x", version="1.0.0", description="d")


def test_orchestrated_with_on_message_raises() -> None:
    class Agent:
        @_mark_on_message
        def handle(self, message: str) -> str:
            return message

    setattr(Agent, ORCHESTRATED_ATTR, {"system_prompt": "x"})

    with pytest.raises(AgentConfigError):
        build_manifest(Agent, name="x", version="1.0.0", description="d")


# ──────────────────────────── validation ────────────────────────────


def test_empty_name_raises() -> None:
    class Agent:
        pass

    with pytest.raises(AgentConfigError):
        build_manifest(Agent, name="", version="1.0.0", description="d")


def test_empty_version_raises() -> None:
    class Agent:
        pass

    with pytest.raises(AgentConfigError):
        build_manifest(Agent, name="x", version="", description="d")


def test_invalid_datasource_raises() -> None:
    class Agent:
        pass

    with pytest.raises(AgentConfigError):
        build_manifest(
            Agent,
            name="x",
            version="1.0.0",
            description="d",
            datasources=("",),  # empty string rejected
        )


def test_duplicate_skill_ids_raise() -> None:
    class Agent:
        @_mark_skill(skill_id="dup")
        def a(self, x: int) -> str:
            return str(x)

        @_mark_skill(skill_id="dup")
        def b(self, x: int) -> str:
            return str(x)

    with pytest.raises(AgentConfigError):
        build_manifest(Agent, name="x", version="1.0.0", description="d")


def test_multiple_on_message_raises() -> None:
    class Agent:
        @_mark_on_message
        def a(self, message: str) -> str:
            return message

        @_mark_on_message
        def b(self, message: str) -> str:
            return message

    with pytest.raises(AgentConfigError):
        find_on_message_handler(Agent)


def test_collect_skills_no_skills() -> None:
    class Agent:
        def regular(self) -> None: ...

    assert collect_skills(Agent) == {}


def test_find_on_message_none() -> None:
    class Agent:
        def regular(self) -> None: ...

    assert find_on_message_handler(Agent) is None


def test_find_orchestrated_none() -> None:
    class Agent:
        pass

    assert find_orchestrated_config(Agent) is None


def test_find_orchestrated_invalid_type_raises() -> None:
    class Agent:
        pass

    setattr(Agent, ORCHESTRATED_ATTR, "not a dict")

    with pytest.raises(AgentConfigError):
        find_orchestrated_config(Agent)


def test_skill_id_defaults_to_method_name() -> None:
    class Agent:
        @_mark_skill(skill_id=None, description="default id")
        def parse_pdf(self, path: str) -> str:
            return path

    registry = collect_skills(Agent)
    assert "parse_pdf" in registry


def test_registry_cached_on_class() -> None:
    class Agent:
        @_mark_skill(skill_id="x")
        def x(self, a: int) -> str:
            return str(a)

    build_manifest(Agent, name="a", version="1.0.0", description="d")
    cached = getattr(Agent, SKILLS_REGISTRY_ATTR)
    assert "x" in cached

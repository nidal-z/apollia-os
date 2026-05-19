"""Tests for ``@agent`` decorator."""

from __future__ import annotations

import asyncio
import sys
import types
from types import SimpleNamespace
from typing import Any

import pytest

from apollia._internal.manifest import (
    AGENT_META_ATTR,
    MANIFEST_ATTR,
    ON_MESSAGE_HANDLER_ATTR,
    SKILLS_REGISTRY_ATTR,
)
from apollia._internal.module_registry import get_module_agent
from apollia.agent import agent
from apollia.errors import AgentConfigError
from apollia.messages import on_message
from apollia.orchestration import orchestrated
from apollia.skills import skill


# ──────────────────────────────────────────────────────────────────────
# Module fixture helper
# ──────────────────────────────────────────────────────────────────────


_MODULE_COUNTER = 0


def _make_module() -> str:
    """Create a unique fake module and return its name."""
    global _MODULE_COUNTER
    _MODULE_COUNTER += 1
    name = f"test_agent_decorator_mod_{_MODULE_COUNTER}"
    sys.modules[name] = types.ModuleType(name)
    return name


def _make_class_in_module(make_fn: Any) -> tuple[str, type]:
    mod_name = _make_module()
    cls = make_fn(mod_name)
    return mod_name, cls


# ──────────────────────────────────────────────────────────────────────
# Manifest + meta caching
# ──────────────────────────────────────────────────────────────────────


def test_agent_manifest_cached_on_class() -> None:
    mod_name = _make_module()

    class A:
        @skill("a.b")
        async def do(self, x: int, ctx: Any = None) -> dict[str, int]:
            return {"x": x}

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="desc")(A)

    manifest = getattr(A, MANIFEST_ATTR)
    assert manifest["name"] == "a"
    assert manifest["version"] == "0.1.0"
    assert manifest["description"] == "desc"
    assert manifest["execution_mode"] == "direct"
    assert manifest["supports_a2a"] is True
    assert len(manifest["skills"]) == 1
    assert manifest["skills"][0]["id"] == "a.b"


def test_agent_meta_cached_on_class() -> None:
    mod_name = _make_module()

    class A:
        @skill("x.y")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    A = agent(
        name="a",
        version="0.1.0",
        description="d",
        packages=("p",),
        tags=("t",),
        datasources=("ds",),
        templates=("tmpl",),
        secrets=("s",),
        tools_required=("tool",),
        user_memory_write=True,
        memory_namespace="ns",
        shared_memory_namespaces=("sh",),
    )(A)

    meta = getattr(A, AGENT_META_ATTR)
    assert meta["name"] == "a"
    assert meta["packages"] == ("p",)
    assert meta["tags"] == ("t",)
    assert meta["datasources"] == ("ds",)
    assert meta["templates"] == ("tmpl",)
    assert meta["secrets"] == ("s",)
    assert meta["tools_required"] == ("tool",)
    assert meta["user_memory_write"] is True
    assert meta["memory_namespace"] == "ns"
    assert meta["shared_memory_namespaces"] == ("sh",)


def test_agent_propagates_metadata_to_manifest() -> None:
    mod_name = _make_module()

    class A:
        @skill("x.y")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    A = agent(
        name="a",
        version="0.1.0",
        description="d",
        packages=("p",),
        tags=("t",),
        datasources=("ds",),
        templates=("tmpl",),
        secrets=("s",),
    )(A)

    m = getattr(A, MANIFEST_ATTR)
    assert m["packages"] == ["p"]
    assert m["tags"] == ["t"]
    assert m["datasources"] == ["ds"]
    assert m["templates"] == ["tmpl"]
    assert m["secrets"] == ["s"]


# ──────────────────────────────────────────────────────────────────────
# Dispatch hook
# ──────────────────────────────────────────────────────────────────────


def test_agent_dispatch_hook_is_async() -> None:
    mod_name = _make_module()

    class A:
        @skill("a.b")
        async def do(self, x: int = 0, ctx: Any = None) -> dict[str, int]:
            return {"x": x}

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)

    assert asyncio.iscoroutinefunction(A.__apollia_dispatch__)  # type: ignore[attr-defined]


def test_agent_dispatch_hook_routes_to_skill() -> None:
    mod_name = _make_module()

    class A:
        @skill("math.add")
        async def add(self, a: int, b: int, ctx: Any = None) -> dict[str, int]:
            return {"sum": a + b}

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)

    inst = A()
    result = asyncio.run(
        inst.__apollia_dispatch__(  # type: ignore[attr-defined]
            {
                "skill_id": "math.add",
                "input": {
                    "parts": [{"type": "data", "data": {"a": 2, "b": 3}}]
                },
            },
            SimpleNamespace(logger=None),
        )
    )
    assert result["status"] == "completed"


# ──────────────────────────────────────────────────────────────────────
# Auto module instance
# ──────────────────────────────────────────────────────────────────────


def test_agent_exposes_instance_to_module() -> None:
    mod_name = _make_module()

    class A:
        @skill("x.y")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)

    instance = get_module_agent(mod_name)
    assert instance is not None
    assert type(instance) is A


def test_agent_two_agents_same_module_raises() -> None:
    mod_name = _make_module()

    class A:
        @skill("x.y")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    agent(name="a", version="0.1.0", description="d")(A)

    class B:
        @skill("x.z")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    B.__module__ = mod_name
    with pytest.raises(AgentConfigError, match="already declares"):
        agent(name="b", version="0.1.0", description="d")(B)


# ──────────────────────────────────────────────────────────────────────
# Required-string validation
# ──────────────────────────────────────────────────────────────────────


def test_agent_empty_name_raises() -> None:
    with pytest.raises(AgentConfigError, match="'name'"):

        @agent(name="", version="0.1.0", description="d")
        class A:
            @skill("x.y")
            async def do(self, ctx: Any = None) -> dict[str, int]:
                return {}


def test_agent_empty_version_raises() -> None:
    with pytest.raises(AgentConfigError, match="'version'"):

        @agent(name="a", version="", description="d")
        class A:
            @skill("x.y")
            async def do(self, ctx: Any = None) -> dict[str, int]:
                return {}


def test_agent_empty_description_raises() -> None:
    with pytest.raises(AgentConfigError, match="'description'"):

        @agent(name="a", version="0.1.0", description="")
        class A:
            @skill("x.y")
            async def do(self, ctx: Any = None) -> dict[str, int]:
                return {}


# ──────────────────────────────────────────────────────────────────────
# __init__ contract
# ──────────────────────────────────────────────────────────────────────


def test_agent_init_with_required_arg_raises() -> None:
    mod_name = _make_module()

    class A:
        def __init__(self, x: int) -> None:
            self.x = x

        @skill("a.b")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    with pytest.raises(AgentConfigError, match="no required"):
        agent(name="a", version="0.1.0", description="d")(A)


def test_agent_init_with_defaults_ok() -> None:
    mod_name = _make_module()

    class A:
        def __init__(self, x: int = 1) -> None:
            self.x = x

        @skill("a.b")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)
    instance = get_module_agent(mod_name)
    assert instance is not None
    assert getattr(instance, "x") == 1


# ──────────────────────────────────────────────────────────────────────
# @orchestrated exclusivity
# ──────────────────────────────────────────────────────────────────────


def test_agent_orchestrated_plus_skill_raises() -> None:
    mod_name = _make_module()

    @orchestrated(system_prompt="research")
    class A:
        @skill("x.y")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    with pytest.raises(AgentConfigError, match="cannot mix @orchestrated"):
        agent(name="a", version="0.1.0", description="d")(A)


def test_agent_orchestrated_plus_on_message_raises() -> None:
    mod_name = _make_module()

    @orchestrated(system_prompt="x")
    class A:
        @on_message
        async def chat(
            self, message: str, history: list, ctx: Any
        ) -> str:
            return "ok"

    A.__module__ = mod_name
    with pytest.raises(AgentConfigError, match="cannot mix @orchestrated"):
        agent(name="a", version="0.1.0", description="d")(A)


def test_agent_pure_orchestrated_ok() -> None:
    mod_name = _make_module()

    @orchestrated(system_prompt="You are a researcher.")
    class A:
        pass

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)
    m = getattr(A, MANIFEST_ATTR)
    assert m["execution_mode"] == "orchestrated"
    assert m["system_prompt"] == "You are a researcher."


# ──────────────────────────────────────────────────────────────────────
# Duplicate handlers
# ──────────────────────────────────────────────────────────────────────


def test_agent_two_on_message_raises() -> None:
    mod_name = _make_module()

    class A:
        @on_message
        async def a(self, message: str, history: list, ctx: Any) -> str:
            return "1"

        @on_message
        async def b(self, message: str, history: list, ctx: Any) -> str:
            return "2"

    A.__module__ = mod_name
    with pytest.raises(AgentConfigError, match="multiple @on_message"):
        agent(name="a", version="0.1.0", description="d")(A)


def test_agent_duplicate_skill_id_raises() -> None:
    mod_name = _make_module()

    class A:
        @skill("dup.id")
        async def a(self, ctx: Any = None) -> dict[str, int]:
            return {}

        @skill("dup.id")
        async def b(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    with pytest.raises(AgentConfigError, match="Duplicate @skill"):
        agent(name="a", version="0.1.0", description="d")(A)


def test_agent_no_handler_raises() -> None:
    mod_name = _make_module()

    class A:
        pass

    A.__module__ = mod_name
    with pytest.raises(AgentConfigError, match="no handler"):
        agent(name="a", version="0.1.0", description="d")(A)


def test_agent_must_decorate_class() -> None:
    deco = agent(name="a", version="0.1.0", description="d")
    with pytest.raises(AgentConfigError, match="must decorate a class"):
        deco("not a class")  # type: ignore[arg-type]


# ──────────────────────────────────────────────────────────────────────
# Tuple validation
# ──────────────────────────────────────────────────────────────────────


def test_agent_packages_must_be_tuple() -> None:
    mod_name = _make_module()

    class A:
        @skill("a.b")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    with pytest.raises(AgentConfigError, match="packages"):
        agent(
            name="a", version="0.1.0", description="d", packages=["bad"]  # type: ignore[arg-type]
        )(A)


def test_agent_datasources_empty_string_raises() -> None:
    mod_name = _make_module()

    class A:
        @skill("a.b")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    with pytest.raises(AgentConfigError, match="datasources"):
        agent(
            name="a",
            version="0.1.0",
            description="d",
            datasources=("",),
        )(A)


def test_agent_conversational_execution_mode() -> None:
    mod_name = _make_module()

    class A:
        @on_message
        async def chat(
            self, message: str, history: list, ctx: Any
        ) -> str:
            return "hi"

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)
    m = getattr(A, MANIFEST_ATTR)
    assert m["execution_mode"] == "conversational"
    assert getattr(A, ON_MESSAGE_HANDLER_ATTR) == "chat"
    assert m["supports_a2a"] is False


def test_agent_skills_registry_populated() -> None:
    mod_name = _make_module()

    class A:
        @skill("a.foo")
        async def foo(self, x: int = 0, ctx: Any = None) -> dict[str, int]:
            return {"x": x}

        @skill("a.bar")
        async def bar(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)
    registry = getattr(A, SKILLS_REGISTRY_ATTR)
    assert set(registry.keys()) == {"a.foo", "a.bar"}

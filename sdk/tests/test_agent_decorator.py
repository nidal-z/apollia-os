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
    # GIVEN a class with one skill, declared in its own module
    mod_name = _make_module()

    class A:
        @skill("a.b")
        async def do(self, x: int, ctx: Any = None) -> dict[str, int]:
            return {"x": x}

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="desc")(A)

    # WHEN the @agent decorator is applied and the manifest is read back
    manifest = getattr(A, MANIFEST_ATTR)
    # THEN the manifest is cached on the class, in direct mode, with that one skill
    assert manifest["name"] == "a"
    assert manifest["version"] == "0.1.0"
    assert manifest["description"] == "desc"
    assert manifest["execution_mode"] == "direct"
    assert manifest["supports_a2a"] is True
    assert len(manifest["skills"]) == 1
    assert manifest["skills"][0]["id"] == "a.b"


def test_agent_meta_cached_on_class() -> None:
    # GIVEN a class declared with every optional metadata field
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

    # WHEN the decorator is applied and the meta attribute is read back
    meta = getattr(A, AGENT_META_ATTR)
    # THEN every field is cached verbatim, tuples still tuples
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
    # GIVEN a class declared with the five list-shaped metadata fields
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

    # WHEN the decorator is applied and the manifest is read back
    m = getattr(A, MANIFEST_ATTR)
    # THEN each tuple reaches the manifest as a list
    assert m["packages"] == ["p"]
    assert m["tags"] == ["t"]
    assert m["datasources"] == ["ds"]
    assert m["templates"] == ["tmpl"]
    assert m["secrets"] == ["s"]


# ──────────────────────────────────────────────────────────────────────
# Dispatch hook
# ──────────────────────────────────────────────────────────────────────


def test_agent_dispatch_hook_is_async() -> None:
    # GIVEN a class with one skill
    # WHEN the decorator is applied and the dispatch hook is inspected
    mod_name = _make_module()

    class A:
        @skill("a.b")
        async def do(self, x: int = 0, ctx: Any = None) -> dict[str, int]:
            return {"x": x}

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)

    # THEN the hook is a coroutine function, awaitable by the bridge
    assert asyncio.iscoroutinefunction(A.__apollia_dispatch__)  # type: ignore[attr-defined]


def test_agent_dispatch_hook_routes_to_skill() -> None:
    # GIVEN a decorated agent exposing one skill
    mod_name = _make_module()

    class A:
        @skill("math.add")
        async def add(self, a: int, b: int, ctx: Any = None) -> dict[str, int]:
            return {"sum": a + b}

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)

    inst = A()
    # WHEN a task naming that skill is dispatched through the hook
    result = asyncio.run(
        inst.__apollia_dispatch__(  # type: ignore[attr-defined]
            {
                "skill_id": "math.add",
                "input": {"parts": [{"type": "data", "data": {"a": 2, "b": 3}}]},
            },
            SimpleNamespace(logger=None),
        )
    )
    # THEN the task completes, so the hook routes to the skill
    assert result["status"] == "completed"


# ──────────────────────────────────────────────────────────────────────
# Auto module instance
# ──────────────────────────────────────────────────────────────────────


def test_agent_exposes_instance_to_module() -> None:
    # GIVEN a class with one skill, declared in its own module
    mod_name = _make_module()

    class A:
        @skill("x.y")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)

    # WHEN the decorator is applied and the module is looked up
    instance = get_module_agent(mod_name)
    # THEN a single instance of that class is exposed at module level
    assert instance is not None
    assert type(instance) is A


def test_agent_two_agents_same_module_raises() -> None:
    # GIVEN a module that already declares one decorated agent
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
    # WHEN a second class in the same module is decorated
    # THEN it is refused, because one module carries one agent
    with pytest.raises(AgentConfigError, match="already declares"):
        agent(name="b", version="0.1.0", description="d")(B)


# ──────────────────────────────────────────────────────────────────────
# Required-string validation
# ──────────────────────────────────────────────────────────────────────


def test_agent_empty_name_raises() -> None:
    # GIVEN a decoration declaring an empty name
    # WHEN the class is decorated
    with pytest.raises(AgentConfigError, match="'name'"):
        # THEN it is refused and the message names the offending field
        @agent(name="", version="0.1.0", description="d")
        class A:
            @skill("x.y")
            async def do(self, ctx: Any = None) -> dict[str, int]:
                return {}


def test_agent_empty_version_raises() -> None:
    # GIVEN a decoration declaring an empty version
    # WHEN the class is decorated
    with pytest.raises(AgentConfigError, match="'version'"):
        # THEN it is refused and the message names the offending field
        @agent(name="a", version="", description="d")
        class A:
            @skill("x.y")
            async def do(self, ctx: Any = None) -> dict[str, int]:
                return {}


def test_agent_empty_description_raises() -> None:
    # GIVEN a decoration declaring an empty description
    # WHEN the class is decorated
    with pytest.raises(AgentConfigError, match="'description'"):
        # THEN it is refused and the message names the offending field
        @agent(name="a", version="0.1.0", description="")
        class A:
            @skill("x.y")
            async def do(self, ctx: Any = None) -> dict[str, int]:
                return {}


# ──────────────────────────────────────────────────────────────────────
# __init__ contract
# ──────────────────────────────────────────────────────────────────────


def test_agent_init_with_required_arg_raises() -> None:
    # GIVEN a class whose __init__ takes a required argument
    mod_name = _make_module()

    class A:
        def __init__(self, x: int) -> None:
            self.x = x

        @skill("a.b")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    # WHEN it is decorated
    # THEN it is refused, because the decorator instantiates with no argument
    with pytest.raises(AgentConfigError, match="no required"):
        agent(name="a", version="0.1.0", description="d")(A)


def test_agent_init_with_defaults_ok() -> None:
    # GIVEN a class whose __init__ arguments all have defaults
    mod_name = _make_module()

    class A:
        def __init__(self, x: int = 1) -> None:
            self.x = x

        @skill("a.b")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)
    # WHEN it is decorated and the module instance is read back
    instance = get_module_agent(mod_name)
    # THEN the instance exists and carries the default value
    assert instance is not None
    assert instance.x == 1


# ──────────────────────────────────────────────────────────────────────
# @orchestrated exclusivity
# ──────────────────────────────────────────────────────────────────────


def test_agent_orchestrated_plus_skill_raises() -> None:
    # GIVEN an orchestrated class that also carries a skill
    mod_name = _make_module()

    @orchestrated(system_prompt="research")
    class A:
        @skill("x.y")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    # WHEN it is decorated
    # THEN it is refused, because the two execution modes are exclusive
    with pytest.raises(AgentConfigError, match="cannot mix @orchestrated"):
        agent(name="a", version="0.1.0", description="d")(A)


def test_agent_orchestrated_plus_on_message_raises() -> None:
    # GIVEN an orchestrated class that also carries a message handler
    mod_name = _make_module()

    @orchestrated(system_prompt="x")
    class A:
        @on_message
        async def chat(self, message: str, history: list, ctx: Any) -> str:
            return "ok"

    A.__module__ = mod_name
    # WHEN it is decorated
    # THEN it is refused, because the two execution modes are exclusive
    with pytest.raises(AgentConfigError, match="cannot mix @orchestrated"):
        agent(name="a", version="0.1.0", description="d")(A)


def test_agent_pure_orchestrated_ok() -> None:
    # GIVEN an orchestrated class with no skill and no message handler
    mod_name = _make_module()

    @orchestrated(system_prompt="You are a researcher.")
    class A:
        pass

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)
    # WHEN it is decorated and the manifest is read back
    m = getattr(A, MANIFEST_ATTR)
    # THEN the mode is orchestrated and the system prompt is carried
    assert m["execution_mode"] == "orchestrated"
    assert m["system_prompt"] == "You are a researcher."


# ──────────────────────────────────────────────────────────────────────
# Duplicate handlers
# ──────────────────────────────────────────────────────────────────────


def test_agent_two_on_message_raises() -> None:
    # GIVEN a class carrying two message handlers
    mod_name = _make_module()

    class A:
        @on_message
        async def a(self, message: str, history: list, ctx: Any) -> str:
            return "1"

        @on_message
        async def b(self, message: str, history: list, ctx: Any) -> str:
            return "2"

    A.__module__ = mod_name
    # WHEN it is decorated
    # THEN it is refused, because a class carries at most one
    with pytest.raises(AgentConfigError, match="multiple @on_message"):
        agent(name="a", version="0.1.0", description="d")(A)


def test_agent_duplicate_skill_id_raises() -> None:
    # GIVEN a class whose two methods claim the same skill id
    mod_name = _make_module()

    class A:
        @skill("dup.id")
        async def a(self, ctx: Any = None) -> dict[str, int]:
            return {}

        @skill("dup.id")
        async def b(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    # WHEN it is decorated
    # THEN it is refused, because dispatch would be ambiguous
    with pytest.raises(AgentConfigError, match="Duplicate @skill"):
        agent(name="a", version="0.1.0", description="d")(A)


def test_agent_no_handler_raises() -> None:
    # GIVEN a class with neither a skill nor a message handler
    mod_name = _make_module()

    class A:
        pass

    A.__module__ = mod_name
    # WHEN it is decorated
    # THEN it is refused, because the minimal contract needs one handler
    with pytest.raises(AgentConfigError, match="no handler"):
        agent(name="a", version="0.1.0", description="d")(A)


def test_agent_must_decorate_class() -> None:
    # GIVEN a valid decorator and a target that is not a class
    deco = agent(name="a", version="0.1.0", description="d")
    # WHEN the decorator is applied to that target
    # THEN it refuses instead of building a manifest for an arbitrary object
    with pytest.raises(AgentConfigError, match="must decorate a class"):
        deco("not a class")  # type: ignore[arg-type]


# ──────────────────────────────────────────────────────────────────────
# Tuple validation
# ──────────────────────────────────────────────────────────────────────


def test_agent_packages_must_be_tuple() -> None:
    # GIVEN a decoration passing packages as a list rather than a tuple
    mod_name = _make_module()

    class A:
        @skill("a.b")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    # WHEN the class is decorated
    # THEN it is refused, so a mutable default cannot reach the manifest
    with pytest.raises(AgentConfigError, match="packages"):
        agent(
            name="a",
            version="0.1.0",
            description="d",
            packages=["bad"],  # type: ignore[arg-type]  # NOSONAR
        )(A)


def test_agent_datasources_empty_string_raises() -> None:
    # GIVEN a decoration declaring an empty string as a datasource
    mod_name = _make_module()

    class A:
        @skill("a.b")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    # WHEN the class is decorated
    # THEN it is refused rather than carried into the manifest
    with pytest.raises(AgentConfigError, match="datasources"):
        agent(
            name="a",
            version="0.1.0",
            description="d",
            datasources=("",),
        )(A)


def test_agent_conversational_execution_mode() -> None:
    # GIVEN a class whose only handler is a message handler
    mod_name = _make_module()

    class A:
        @on_message
        async def chat(self, message: str, history: list, ctx: Any) -> str:
            return "hi"

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)
    # WHEN it is decorated and the manifest is read back
    m = getattr(A, MANIFEST_ATTR)
    # THEN the mode is conversational, the handler is named, and a2a is off
    assert m["execution_mode"] == "conversational"
    assert getattr(A, ON_MESSAGE_HANDLER_ATTR) == "chat"
    assert m["supports_a2a"] is False


def test_agent_skills_registry_populated() -> None:
    # GIVEN a class carrying two skills
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
    # WHEN it is decorated and the registry is read back
    registry = getattr(A, SKILLS_REGISTRY_ATTR)
    # THEN both skill ids are in the registry dispatch will read
    assert set(registry.keys()) == {"a.foo", "a.bar"}


# ──────────────────────────────────────────────────────────────────────
# autonomy_level
# ──────────────────────────────────────────────────────────────────────


def test_autonomy_level_supervised_accepted() -> None:
    # GIVEN an agent declaring autonomy_level="supervised"
    mod_name = _make_module()

    class A:
        @skill("a.b")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    # WHEN the decorator is applied with that tier
    A = agent(name="a", version="0.1.0", description="d", autonomy_level="supervised")(A)

    # THEN the manifest carries the tier
    manifest = getattr(A, MANIFEST_ATTR)
    assert manifest["autonomy_level"] == "supervised"


def test_autonomy_level_none_absent_from_manifest() -> None:
    # GIVEN an agent without autonomy_level
    mod_name = _make_module()

    class A:
        @skill("a.b")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    # WHEN the decorator is applied without a tier
    A = agent(name="a", version="0.1.0", description="d")(A)

    # THEN the key is absent (not null) so the loader uses its default
    manifest = getattr(A, MANIFEST_ATTR)
    assert "autonomy_level" not in manifest


def test_autonomy_level_invalid_raises() -> None:
    # GIVEN an invalid tier
    mod_name = _make_module()

    class A:
        @skill("a.b")
        async def do(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name

    # WHEN the decorator is applied with a tier outside the accepted set
    # THEN the decorator fails fast at import
    with pytest.raises(AgentConfigError, match="autonomy_level"):
        agent(name="a", version="0.1.0", description="d", autonomy_level="ultra_autonomous")(A)


def test_all_valid_autonomy_levels_accepted() -> None:
    from apollia.agent import _check_autonomy_level

    # GIVEN each of the four valid tiers
    for level in ("assisted", "supervised", "bounded_autonomous", "long_autonomous"):
        # WHEN the tier is validated
        # THEN validation returns it unchanged
        assert _check_autonomy_level(level) == level
    # AND None is accepted (no tier declared)
    assert _check_autonomy_level(None) is None


def test_valid_autonomy_levels_frozenset_contains_all_four() -> None:
    # GIVEN the frozenset of accepted autonomy tiers
    from apollia.agent import _VALID_AUTONOMY_LEVELS

    # WHEN it is compared with the four documented tiers
    # THEN they match exactly, so no fifth tier is silently accepted
    assert {
        "assisted",
        "supervised",
        "bounded_autonomous",
        "long_autonomous",
    } == _VALID_AUTONOMY_LEVELS

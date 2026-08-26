"""Tests for the module-level agent exposure helper."""

from __future__ import annotations

import sys
import types
from typing import Any, ClassVar

import pytest
from apollia._internal.module_registry import expose_to_module, get_module_agent
from apollia.errors import AgentConfigError


def _make_module(name: str) -> types.ModuleType:
    module = types.ModuleType(name)
    sys.modules[name] = module
    return module


def _make_agent_in_module(module: types.ModuleType) -> tuple[type, Any]:
    class Agent:
        # Simulate the marker set by the @agent decorator.
        __apollia_manifest__: ClassVar[dict[str, Any]] = {"name": "test", "version": "0.0.0"}

    Agent.__module__ = module.__name__
    setattr(module, Agent.__name__, Agent)
    return Agent, Agent()


def test_expose_to_module_sets_agent() -> None:
    # GIVEN a module holding an agent class and one instance of it
    module = _make_module("apollia_test_module_registry_a")
    cls, instance = _make_agent_in_module(module)

    # WHEN the instance is exposed
    expose_to_module(cls, instance)

    # THEN the module-level `agent` name points at that instance
    assert module.agent is instance


def test_expose_to_module_idempotent_same_instance() -> None:
    # GIVEN a module holding an agent class and one instance of it
    module = _make_module("apollia_test_module_registry_b")
    cls, instance = _make_agent_in_module(module)

    # WHEN the same instance is exposed twice
    expose_to_module(cls, instance)
    expose_to_module(cls, instance)

    # THEN the second call is a no-op rather than a collision
    assert module.agent is instance


def test_expose_to_module_replaces_same_class() -> None:
    # GIVEN a module holding an agent class and two instances of it
    module = _make_module("apollia_test_module_registry_c")
    cls, instance = _make_agent_in_module(module)
    other = cls()

    # WHEN both are exposed in turn
    expose_to_module(cls, instance)
    expose_to_module(cls, other)

    # THEN the last instance of the same class wins
    assert module.agent is other


def test_expose_to_module_different_agent_class_raises() -> None:
    """Real agent instance collision (both classes carry __apollia_manifest__) raises."""
    # GIVEN a module holding two distinct agent classes, one already exposed
    module = _make_module("apollia_test_module_registry_d")
    cls_a, instance_a = _make_agent_in_module(module)

    class OtherAgent:
        __apollia_manifest__: ClassVar[dict[str, Any]] = {"name": "other", "version": "0.0.0"}

    OtherAgent.__module__ = module.__name__
    module.OtherAgent = OtherAgent

    expose_to_module(cls_a, instance_a)

    # WHEN the second agent class tries to take the same module-level name
    # THEN it is refused, because one module carries one agent
    with pytest.raises(AgentConfigError):
        expose_to_module(OtherAgent, OtherAgent())


def test_expose_to_module_overwrites_non_agent_shadow() -> None:
    """If module.agent is a decorator/callable/foreign object (no
    __apollia_manifest__), it is silently overwritten - this is the case
    when the user does `from apollia import agent` at the top of the file."""
    # GIVEN a module whose `agent` name holds the imported decorator, not an agent
    module = _make_module("apollia_test_module_registry_shadow")

    def fake_decorator(*args: Any, **kwargs: Any) -> Any:
        return None

    module.agent = fake_decorator  # simulates `from apollia import agent`
    cls, instance = _make_agent_in_module(module)

    # WHEN a real agent instance is exposed
    expose_to_module(cls, instance)

    # THEN the shadow is overwritten instead of raising a collision
    assert module.agent is instance  # silently replaced


def test_get_module_agent_present() -> None:
    # GIVEN a module with an exposed agent instance
    module = _make_module("apollia_test_module_registry_e")
    cls, instance = _make_agent_in_module(module)
    expose_to_module(cls, instance)

    # WHEN the module is looked up by name
    # THEN the exposed instance comes back
    assert get_module_agent(module.__name__) is instance


def test_get_module_agent_missing() -> None:
    # GIVEN a module name that was never imported
    # WHEN it is looked up
    # THEN the lookup renders None instead of raising
    assert get_module_agent("apollia_test_does_not_exist") is None


def test_get_module_agent_module_present_but_no_agent() -> None:
    # GIVEN an imported module that exposes no agent
    module = _make_module("apollia_test_module_registry_f")

    # WHEN it is looked up by name
    # THEN the lookup renders None instead of raising
    assert get_module_agent(module.__name__) is None


def test_expose_to_module_unresolvable_module() -> None:
    # GIVEN a class whose declared module cannot be resolved
    class Floating:
        pass

    # Force inspect.getmodule to fail by giving an unknown module name.
    Floating.__module__ = "definitely.not.a.real.module"

    # WHEN an instance of it is exposed
    # THEN the helper fails loudly instead of writing nowhere
    with pytest.raises(AgentConfigError):
        expose_to_module(Floating, Floating())

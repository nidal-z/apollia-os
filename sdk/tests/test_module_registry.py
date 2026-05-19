"""Tests for the module-level agent exposure helper."""

from __future__ import annotations

import sys
import types
from typing import Any

import pytest

from apollia._internal.module_registry import expose_to_module, get_module_agent
from apollia.errors import AgentConfigError


def _make_module(name: str) -> types.ModuleType:
    module = types.ModuleType(name)
    sys.modules[name] = module
    return module


def _make_agent_in_module(module: types.ModuleType) -> tuple[type, Any]:
    class Agent:
        pass

    Agent.__module__ = module.__name__
    setattr(module, Agent.__name__, Agent)
    return Agent, Agent()


def test_expose_to_module_sets_agent() -> None:
    module = _make_module("apollia_test_module_registry_a")
    cls, instance = _make_agent_in_module(module)

    expose_to_module(cls, instance)
    assert module.agent is instance


def test_expose_to_module_idempotent_same_instance() -> None:
    module = _make_module("apollia_test_module_registry_b")
    cls, instance = _make_agent_in_module(module)

    expose_to_module(cls, instance)
    expose_to_module(cls, instance)
    assert module.agent is instance


def test_expose_to_module_replaces_same_class() -> None:
    module = _make_module("apollia_test_module_registry_c")
    cls, instance = _make_agent_in_module(module)
    other = cls()

    expose_to_module(cls, instance)
    expose_to_module(cls, other)
    assert module.agent is other


def test_expose_to_module_different_class_raises() -> None:
    module = _make_module("apollia_test_module_registry_d")
    cls_a, instance_a = _make_agent_in_module(module)

    class OtherAgent:
        pass

    OtherAgent.__module__ = module.__name__
    setattr(module, "OtherAgent", OtherAgent)

    expose_to_module(cls_a, instance_a)
    with pytest.raises(AgentConfigError):
        expose_to_module(OtherAgent, OtherAgent())


def test_get_module_agent_present() -> None:
    module = _make_module("apollia_test_module_registry_e")
    cls, instance = _make_agent_in_module(module)
    expose_to_module(cls, instance)

    assert get_module_agent(module.__name__) is instance


def test_get_module_agent_missing() -> None:
    assert get_module_agent("apollia_test_does_not_exist") is None


def test_get_module_agent_module_present_but_no_agent() -> None:
    module = _make_module("apollia_test_module_registry_f")
    assert get_module_agent(module.__name__) is None


def test_expose_to_module_unresolvable_module() -> None:
    class Floating:
        pass

    # Force inspect.getmodule to fail by giving an unknown module name.
    Floating.__module__ = "definitely.not.a.real.module"
    with pytest.raises(AgentConfigError):
        expose_to_module(Floating, Floating())

"""Tests for the ``Ctx`` Protocol and its 14 typed sub-surfaces."""

from __future__ import annotations

from typing import Any
from unittest.mock import MagicMock

import pytest

# Importability


def test_ctx_importable_from_apollia_types() -> None:
    # GIVEN the typed contract module
    # WHEN Ctx is imported from it
    # THEN the import succeeds, so the documented path is real
    from apollia.types import Ctx  # noqa: F401


def test_ctx_importable_from_apollia_root() -> None:
    # GIVEN the package root
    # WHEN Ctx is imported from it
    # THEN the import succeeds, so the short path is real too
    from apollia import Ctx  # noqa: F401


def test_each_subprotocol_importable() -> None:
    """All 14 sub-protocols are exported from ``apollia.context.*``."""
    # GIVEN the context package
    # WHEN every sub-protocol is imported from its own module
    # THEN each import succeeds, so no surface is exported by name only
    from apollia.context.a2a import A2AInterface  # noqa: F401
    from apollia.context.budget import BudgetView  # noqa: F401
    from apollia.context.datasources import DatasourcesInterface  # noqa: F401
    from apollia.context.events import EventsInterface  # noqa: F401
    from apollia.context.llm import LlmProxy, LlmResponse, TokenUsage  # noqa: F401
    from apollia.context.logger import Logger  # noqa: F401
    from apollia.context.mail import MailInterface  # noqa: F401
    from apollia.context.memory import MemoryInterface  # noqa: F401
    from apollia.context.notify import NotifyInterface  # noqa: F401
    from apollia.context.profile import ProfileInterface  # noqa: F401
    from apollia.context.secrets import SecretsInterface  # noqa: F401
    from apollia.context.stt import SttInterface  # noqa: F401
    from apollia.context.templates import TemplatesInterface  # noqa: F401
    from apollia.context.tools import ToolDescriptor, ToolProxy  # noqa: F401
    from apollia.context.workspace import WorkspaceContext  # noqa: F401


# ──────────────────────────────────────────────────────────────────────
# Each sub-Protocol is runtime_checkable (except Logger which is concrete)
# ──────────────────────────────────────────────────────────────────────


@pytest.mark.parametrize(
    ("module_path", "name"),
    [
        ("apollia.context.llm", "LlmProxy"),
        ("apollia.context.llm", "LlmResponse"),
        ("apollia.context.llm", "TokenUsage"),
        ("apollia.context.memory", "MemoryInterface"),
        ("apollia.context.tools", "ToolProxy"),
        ("apollia.context.tools", "ToolDescriptor"),
        ("apollia.context.a2a", "A2AInterface"),
        ("apollia.context.a2a", "SkillCard"),
        ("apollia.context.datasources", "DatasourcesInterface"),
        ("apollia.context.templates", "TemplatesInterface"),
        ("apollia.context.secrets", "SecretsInterface"),
        ("apollia.context.events", "EventsInterface"),
        ("apollia.context.profile", "ProfileInterface"),
        ("apollia.context.workspace", "WorkspaceContext"),
        ("apollia.context.stt", "SttInterface"),
        ("apollia.context.mail", "MailInterface"),
        ("apollia.context.notify", "NotifyInterface"),
        ("apollia.context.budget", "BudgetView"),
    ],
)
def test_subprotocol_is_runtime_checkable(module_path: str, name: str) -> None:
    """Each sub-protocol can be used with ``isinstance()``."""
    import importlib

    # GIVEN one sub-protocol named by the parametrisation
    mod = importlib.import_module(module_path)

    # WHEN its runtime-protocol marker is read
    proto = getattr(mod, name)

    # THEN the marker is set, so isinstance() against it is legal
    # ``runtime_checkable`` Protocols pass an empty object as not an instance,
    # but the marker attribute exists on the class.
    assert getattr(proto, "_is_runtime_protocol", False), f"{name} is not @runtime_checkable"


def test_ctx_is_runtime_checkable() -> None:
    # GIVEN the aggregate Ctx protocol
    from apollia.types import Ctx

    # WHEN its runtime-protocol marker is read
    # THEN the marker is set, so an agent can assert on the whole context
    assert getattr(Ctx, "_is_runtime_protocol", False)


# ──────────────────────────────────────────────────────────────────────
# isinstance() semantics
# ──────────────────────────────────────────────────────────────────────


class _CompleteCtxMock:
    """A bare-bones object exposing all 14 ``Ctx`` attributes."""

    def __init__(self) -> None:
        self.llm: Any = MagicMock()
        self.memory: Any = MagicMock()
        self.tools: Any = MagicMock()
        self.a2a: Any = MagicMock()
        self.mail: Any = MagicMock()
        self.datasources: Any = MagicMock()
        self.templates: Any = MagicMock()
        self.secrets: Any = MagicMock()
        self.events: Any = MagicMock()
        self.logger: Any = MagicMock()
        self.profile: Any = MagicMock()
        self.workspace: Any = MagicMock()
        self.stt: Any = MagicMock()
        self.notify: Any = MagicMock()
        self.budget: Any = MagicMock()


class _IncompleteCtxMock:
    """Same as :class:`_CompleteCtxMock` but missing ``llm``."""

    def __init__(self) -> None:
        # Intentionally NOT setting ``llm``.
        self.memory: Any = MagicMock()
        self.tools: Any = MagicMock()
        self.a2a: Any = MagicMock()
        self.mail: Any = MagicMock()
        self.datasources: Any = MagicMock()
        self.templates: Any = MagicMock()
        self.secrets: Any = MagicMock()
        self.events: Any = MagicMock()
        self.logger: Any = MagicMock()
        self.profile: Any = MagicMock()
        self.workspace: Any = MagicMock()
        self.stt: Any = MagicMock()
        self.notify: Any = MagicMock()
        self.budget: Any = MagicMock()


def test_complete_mock_satisfies_ctx() -> None:
    # GIVEN an object exposing every Ctx attribute
    from apollia.types import Ctx

    mock = _CompleteCtxMock()

    # WHEN it is checked against the protocol
    # THEN it satisfies it, so duck typing is enough to be a context
    assert isinstance(mock, Ctx)


def test_incomplete_mock_does_not_satisfy_ctx() -> None:
    """An object missing a required attribute fails ``isinstance(Ctx)``."""
    # GIVEN an object missing the llm attribute
    from apollia.types import Ctx

    mock = _IncompleteCtxMock()

    # WHEN it is checked against the protocol
    # THEN it fails, so a partial context is not silently accepted
    assert not isinstance(mock, Ctx)


def test_unrelated_object_not_ctx() -> None:
    # GIVEN objects that expose none of the Ctx attributes
    from apollia.types import Ctx

    # WHEN each is checked against the protocol
    # THEN none satisfies it
    assert not isinstance(object(), Ctx)
    assert not isinstance("hello", Ctx)
    assert not isinstance(42, Ctx)


# ──────────────────────────────────────────────────────────────────────
# Logger is the stdlib logger, not a Protocol
# ──────────────────────────────────────────────────────────────────────


def test_logger_is_stdlib_logger() -> None:
    import logging

    # GIVEN the Logger name exported by the context package
    from apollia.context.logger import Logger

    # WHEN it is compared with the stdlib class
    # THEN it is the same class, not a protocol standing in for it
    assert Logger is logging.Logger


# ──────────────────────────────────────────────────────────────────────
# Spot check sub-Protocol attribute presence (Protocol introspection)
# ──────────────────────────────────────────────────────────────────────


def test_memory_protocol_has_expected_methods() -> None:
    # GIVEN the memory sub-protocol and the method set the SDK documents
    from apollia.context.memory import MemoryInterface

    expected = {
        "record",
        "remember",
        "recall",
        "recall_entry",
        "recall_all",
        "forget",
        "search",
        "learn_procedure",
        "recall_procedure",
        "export",
        "import_data",
    }

    # WHEN the protocol members are listed
    members = set(dir(MemoryInterface))
    missing = expected - members

    # THEN none of the documented methods is absent
    assert not missing, f"MemoryInterface missing: {missing}"


def test_a2a_protocol_has_invoke_discover_list() -> None:
    # GIVEN the A2A sub-protocol
    from apollia.context.a2a import A2AInterface

    # WHEN its members are listed
    members = set(dir(A2AInterface))

    # THEN the four documented entry points are all present
    assert "invoke" in members
    assert "discover" in members
    assert "list_skills" in members
    assert "skill_as_tool" in members

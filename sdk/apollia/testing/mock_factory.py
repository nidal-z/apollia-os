"""Factory for ``(agent_instance, MockContext)`` used in unit tests.

See LOT 10 of the SDK refactor.  This module is the public entry point
for the ``apollia.testing`` package: ``from apollia.testing import mock``.

Usage::

    from apollia.testing import mock, assert_skill_called

    agent, ctx = mock(MyAgent)
    ctx.llm.responses = [{"content": "..."}]
    result = await agent.invoke_skill("my.skill", x=1)
    assert_skill_called(ctx, "child.skill")
"""

from __future__ import annotations

import logging
from typing import Any, TypeVar

from apollia._internal.dispatch import dispatch_message, dispatch_skill
from apollia._internal.manifest import MANIFEST_ATTR
from apollia.testing.context import (
    MockA2A,
    MockBudget,
    MockDatasources,
    MockEvents,
    MockNotify,
    MockProfile,
    MockSecrets,
    MockStt,
    MockTemplates,
    MockWorkspace,
)
from apollia.testing.mocks import MockLlmProxy, MockMemory, MockToolProxy

T = TypeVar("T")


class MockContext:
    """Mock of the full ``Ctx`` Protocol — all 14 service surfaces.

    The constructor wires fresh instances of every ``Mock<X>`` service.
    Pre-configure each surface in place::

        ctx.llm.responses = [{"content": "..."}]
        ctx.datasources.values = {"competitors": [...]}
        ctx.secrets.values = {"api_key": "sk-..."}
        ctx.templates.templates = {"prompt.j2": "Hello {{ name }}"}

    Tests assert on the same surfaces::

        assert ctx.events.tokens == ["Hello", " world"]
        assert ctx.a2a.invoke_calls == [("worker.skill", {"x": 1})]
    """

    def __init__(self) -> None:
        self.llm = MockLlmProxy()
        self.memory = MockMemory()
        self.tools = MockToolProxy()
        self.a2a = MockA2A()
        self.datasources = MockDatasources()
        self.templates = MockTemplates()
        self.secrets = MockSecrets()
        self.events = MockEvents()
        self.profile = MockProfile()
        self.workspace = MockWorkspace()
        self.stt = MockStt()
        self.notify = MockNotify()
        self.budget = MockBudget()
        self.logger: logging.Logger = logging.getLogger("apollia.test.agent")

    def log(self, level: str, message: str) -> None:
        """Compatibility shim for code that calls ``ctx.log(level, msg)``."""
        self.logger.log(
            getattr(logging, level.upper(), logging.INFO), message
        )


def mock(agent_cls: type[T]) -> tuple[T, MockContext]:
    """Create an agent instance bound to a fully-mocked ``Ctx``.

    The returned agent is decorated with two extra helpers that bypass
    the Rust runtime and dispatch handlers directly with the mock ctx:

    * ``await agent.invoke_skill(skill_id, **payload)`` →
      :class:`AIPResult` dict (handles validation, ``ctx=`` injection,
      exception trapping).
    * ``await agent.invoke_message(message, history=None)`` →
      :class:`AIPResult` dict via the agent's ``@on_message`` handler.

    Args:
        agent_cls: A class decorated with :func:`apollia.agent`.

    Returns:
        Tuple of ``(agent_instance, mock_ctx)``.  Configure ``mock_ctx``
        before calling the helpers.

    Raises:
        ValueError: If ``agent_cls`` is not decorated with ``@agent``.
    """
    if not hasattr(agent_cls, MANIFEST_ATTR):
        raise ValueError(
            f"{agent_cls.__name__} is not decorated with @agent — "
            f"missing attribute {MANIFEST_ATTR!r}"
        )

    instance = agent_cls()
    ctx = MockContext()

    async def invoke_skill(
        skill_id: str, **kwargs: Any
    ) -> dict[str, Any]:
        """Invoke a ``@skill`` handler directly with the mock ctx."""
        return await dispatch_skill(instance, skill_id, kwargs, ctx)

    async def invoke_message(
        message: str,
        history: list[dict[str, Any]] | None = None,
    ) -> dict[str, Any]:
        """Invoke the ``@on_message`` handler directly with the mock ctx."""
        return await dispatch_message(
            instance, message, history or [], ctx
        )

    instance.invoke_skill = invoke_skill  # type: ignore[attr-defined]
    instance.invoke_message = invoke_message  # type: ignore[attr-defined]

    return instance, ctx


__all__ = ["MockContext", "mock"]

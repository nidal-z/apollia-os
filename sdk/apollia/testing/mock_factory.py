"""``MockContext``, the ``ctx`` a tool or a workflow receives in a unit test.

This module is the public entry point for the ``apollia.testing``
package: ``from apollia.testing import MockContext``.

A tool and a workflow are plain ``async def`` functions: a test builds the
context, configures its services and awaits the function itself::

    from apollia.testing import MockContext, assert_tool_called

    ctx = MockContext()
    ctx.llm.responses = [{"content": "..."}]
    result = await my_workflow(ctx, {"path": "notes.txt"})
    assert_tool_called(ctx, "file_read")
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

from apollia.testing.context import (
    MockAgents,
    MockBudget,
    MockDatasources,
    MockEvents,
    MockNotify,
    MockProfile,
    MockSecrets,
    MockStt,
    MockTemplates,
    MockUser,
    MockWorkflows,
    MockWorkspace,
)
from apollia.testing.mocks import MockLlmProxy, MockMemory, MockToolProxy

if TYPE_CHECKING:
    from apollia.hitl import HitlPayload, InputResponse


class MockContext:
    """Mock of the ``Ctx`` Protocol: every service surface but ``mail``.

    The constructor wires fresh instances of every ``Mock<X>`` service.
    Pre-configure each surface in place::

        ctx.llm.responses = [{"content": "..."}]
        ctx.datasources.values = {"competitors": [...]}
        ctx.secrets.values = {"api_key": "sk-..."}
        ctx.templates.templates = {"prompt.j2": "Hello {{ name }}"}

    Tests assert on the same surfaces::

        assert ctx.events.tokens == ["Hello", " world"]
        assert ctx.agents.run_calls == [("triage", "Sort this message.")]
    """

    def __init__(self) -> None:
        """Wire a fresh mock for every ``ctx`` service surface."""
        self.llm = MockLlmProxy()
        self.memory = MockMemory()
        self.tools = MockToolProxy()
        self.agents = MockAgents()
        self.workflows = MockWorkflows()
        self.user = MockUser()
        self.datasources = MockDatasources()
        self.templates = MockTemplates()
        self.secrets = MockSecrets()
        self.events = MockEvents()
        self.profile = MockProfile()
        self.workspace = MockWorkspace()
        self.stt = MockStt()
        self.notify = MockNotify()
        self.budget = MockBudget()
        self.logger: logging.Logger = logging.getLogger("apollia.test")
        # A first run until a test says otherwise, as in production.
        self.is_resumed: bool = False
        self.input_response: InputResponse | None = None

    def resume_with(
        self,
        *,
        approved: bool = True,
        answer: str | float | bool | None = None,
        reason: str | None = None,
        payload: HitlPayload | None = None,
        context: dict[str, Any] | None = None,
    ) -> None:
        """Put the context in the state of a task resumed from a pause.

        The runtime runs resumed code again from the top with
        ``ctx.is_resumed`` true and ``ctx.input_response`` set; this does the
        same for a test, so the resume branch can be exercised without a
        runtime::

            ctx = MockContext()
            with pytest.raises(NeedHumanInput) as paused:
                await pick(ctx)
            ctx.resume_with(answer="b", payload=paused.value.payload)
            result = await pick(ctx)

        Args:
            approved: The operator's decision.
            answer: The answer to a question: a proposition id, free text or a
                value. Leave ``None`` for an approval.
            reason: The operator's reason, if any.
            payload: The pause being answered, usually the ``payload`` of the
                ``NeedHumanInput`` the first run raised.
            context: The ``context`` that pause carried.
        """
        self.is_resumed = True
        self.input_response = {
            "approved": approved,
            "reason": reason,
            "answer": answer,
            "payload": payload,
            "context": context if context is not None else {},
        }

    def log(self, level: str, message: str) -> None:
        """Compatibility shim for code that calls ``ctx.log(level, msg)``."""
        self.logger.log(getattr(logging, level.upper(), logging.INFO), message)


__all__ = ["MockContext"]

"""Apollia SDK testing utilities.

The recommended entry point is :func:`mock`::

    from apollia.testing import mock, assert_result_completed, assert_skill_called

    agent, ctx = mock(MyAgent)
    ctx.llm.responses = [{"content": "answer"}]
    result = await agent.invoke_skill("my.skill", x=1)
    assert_result_completed(result)
    assert_skill_called(ctx, "child.worker")
"""

from apollia.testing.assertions import (
    assert_emitted_thought,
    assert_emitted_token,
    assert_llm_called,
    assert_memory_recorded,
    assert_result_completed,
    assert_result_failed,
    assert_result_input_required,
    assert_skill_called,
    assert_template_rendered,
    assert_tool_called,
)
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
from apollia.testing.mock_factory import MockContext, mock
from apollia.testing.mocks import (
    MockLlmProxy,
    MockLlmResponse,
    MockMemory,
    MockToolProxy,
)

__all__ = [
    # Per-surface mocks (advanced usage)
    "MockA2A",
    "MockBudget",
    "MockContext",
    "MockDatasources",
    "MockEvents",
    "MockLlmProxy",
    "MockLlmResponse",
    "MockMemory",
    "MockNotify",
    "MockProfile",
    "MockSecrets",
    "MockStt",
    "MockTemplates",
    "MockToolProxy",
    "MockWorkspace",
    # Interaction assertions
    "assert_emitted_thought",
    "assert_emitted_token",
    "assert_llm_called",
    "assert_memory_recorded",
    # AIPResult assertions
    "assert_result_completed",
    "assert_result_failed",
    "assert_result_input_required",
    "assert_skill_called",
    "assert_template_rendered",
    "assert_tool_called",
    # Factory + main context
    "mock",
]

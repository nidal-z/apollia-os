"""Apollia SDK testing utilities.

The entry point is :class:`MockContext`, the ``ctx`` a tool or a workflow
receives. Both are plain ``async def`` functions, awaited as written::

    from apollia.testing import MockContext, assert_llm_called

    ctx = MockContext()
    ctx.llm.responses = [{"content": "answer"}]
    result = await my_workflow(ctx, {"path": "notes.txt"})
    assert_llm_called(ctx, times=1)
"""

from apollia.testing.assertions import (
    assert_emitted_thought,
    assert_emitted_token,
    assert_llm_called,
    assert_memory_recorded,
    assert_result_completed,
    assert_result_failed,
    assert_result_input_required,
    assert_template_rendered,
    assert_tool_called,
)
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
from apollia.testing.mock_factory import MockContext
from apollia.testing.mocks import (
    MockLlmProxy,
    MockLlmResponse,
    MockMemory,
    MockToolProxy,
)

__all__ = [
    # Per-surface mocks (advanced usage)
    "MockAgents",
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
    "MockUser",
    "MockWorkflows",
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
    "assert_template_rendered",
    "assert_tool_called",
]

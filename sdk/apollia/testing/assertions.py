"""Assertion helpers for Apollia agent test suites.

Provides specialised assertion functions that produce clear, contextual
error messages when verifying agent execution results and mock
interactions.  All functions are synchronous and raise ``AssertionError``
on failure, making them directly compatible with pytest.

Example::

    from apollia.testing import MockContext, assert_result_completed, assert_tool_called

    result = await my_agent.run(task, ctx)
    assert_result_completed(result, contains="Hello")
    assert_tool_called(ctx, "bash", times=1)
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from apollia.utils.formatting import aip_result_text

if TYPE_CHECKING:
    from apollia.testing.mocks import MockContext

_TEXT_PREVIEW_LIMIT = 100


def assert_result_completed(
    result: dict[str, Any],
    contains: str | None = None,
) -> None:
    """Verify that an AIP result has status ``completed``.

    Args:
        result: AIPResult dictionary to verify.
        contains: If provided, additionally check that the concatenated
            text parts contain this substring.

    Raises:
        AssertionError: If the status is not ``completed`` or the text
            does not contain the expected substring.
    """
    _assert_status(result, "completed")

    if contains is not None:
        text = aip_result_text(result)
        if contains not in text:
            preview = text[:_TEXT_PREVIEW_LIMIT]
            raise AssertionError(
                f"Expected result text to contain '{contains}', got: '{preview}'"
            )


def assert_result_failed(
    result: dict[str, Any],
    code: str | None = None,
) -> None:
    """Verify that an AIP result has status ``failed``.

    Args:
        result: AIPResult dictionary to verify.
        code: If provided, additionally check that the error code matches.

    Raises:
        AssertionError: If the status is not ``failed`` or the error code
            does not match.
    """
    _assert_status(result, "failed")

    if code is not None:
        error = result.get("error", {})
        actual_code = error.get("code", "")
        if actual_code != code:
            raise AssertionError(
                f"Expected error code '{code}', got '{actual_code}'"
            )


def assert_result_input_required(result: dict[str, Any]) -> None:
    """Verify that an AIP result has status ``input_required``.

    Args:
        result: AIPResult dictionary to verify.

    Raises:
        AssertionError: If the status is not ``input_required``.
    """
    _assert_status(result, "input_required")


def assert_tool_called(
    ctx: MockContext,
    tool_name: str,
    times: int | None = None,
) -> None:
    """Verify that a tool was called via the ``MockContext``.

    Args:
        ctx: ``MockContext`` containing a ``MockToolProxy``.
        tool_name: Name of the tool to verify.
        times: If provided, verify the exact number of calls.

    Raises:
        AssertionError: If the tool was never called, or not the expected
            number of times, or if tools are not configured.
    """
    if ctx.tools is None:
        raise AssertionError("MockContext has no tools configured (tools is None)")

    actual_count = sum(1 for name, _ in ctx.tools.calls if name == tool_name)

    if times is None:
        if actual_count == 0:
            raise AssertionError(
                f"Expected tool '{tool_name}' to be called, but it was never called"
            )
    elif actual_count != times:
        raise AssertionError(
            f"Expected tool '{tool_name}' to be called {times} times, "
            f"was called {actual_count} times"
        )


def assert_llm_called(
    ctx: MockContext,
    times: int | None = None,
) -> None:
    """Verify that the LLM was called via the ``MockContext``.

    Args:
        ctx: ``MockContext`` containing a ``MockLlmProxy``.
        times: If provided, verify the exact number of calls.

    Raises:
        AssertionError: If the LLM was never called, or not the expected
            number of times, or if LLM is not configured.
    """
    if ctx.llm is None:
        raise AssertionError("MockContext has no LLM configured (llm is None)")

    actual_count = ctx.llm.call_count

    if times is None:
        if actual_count == 0:
            raise AssertionError(
                "Expected LLM to be called, but it was never called"
            )
    elif actual_count != times:
        raise AssertionError(
            f"Expected LLM to be called {times} times, "
            f"was called {actual_count} times"
        )


def _assert_status(result: dict[str, Any], expected: str) -> None:
    """Verify the ``status`` field of an AIP result dictionary.

    Raises:
        AssertionError: If the key is missing or the value differs.
    """
    actual = result.get("status")
    if actual is None:
        raise AssertionError(
            f"Expected result status '{expected}', but 'status' key is missing"
        )
    if actual != expected:
        raise AssertionError(
            f"Expected result status '{expected}', got '{actual}'"
        )

"""Tests for apollia.testing.assertions - assertion helpers for agent tests."""

from __future__ import annotations

import pytest
from apollia.testing import (
    MockContext,
    assert_llm_called,
    assert_result_completed,
    assert_result_failed,
    assert_result_input_required,
    assert_tool_called,
)


def _completed_result(text: str = "ok") -> dict:
    return {"status": "completed", "output": [{"type": "text", "text": text}]}


def _failed_result(code: str = "TOOL_ERROR") -> dict:
    return {"status": "failed", "error": {"code": code}}


def _input_required_result() -> dict:
    return {"status": "input_required", "input": {"prompt": "Approve?"}}


class TestAssertResultCompleted:
    """Tests for assert_result_completed."""

    def test_passes_on_completed_result(self) -> None:
        # GIVEN a completed result
        result = _completed_result()
        # WHEN / THEN no exception
        assert_result_completed(result)

    def test_fails_with_message_on_wrong_status(self) -> None:
        # GIVEN a failed result
        result = _failed_result()
        # WHEN assert_result_completed is called
        # THEN AssertionError with explicit message
        with pytest.raises(
            AssertionError,
            match="Expected result status 'completed', got 'failed'",
        ):
            assert_result_completed(result)

    def test_contains_passes_when_substring_found(self) -> None:
        # GIVEN a completed result with text "Hello World"
        result = _completed_result("Hello World")
        # WHEN / THEN no exception
        assert_result_completed(result, contains="Hello")

    def test_contains_fails_when_substring_missing(self) -> None:
        # GIVEN a completed result with text "Hello World"
        result = _completed_result("Hello World")
        # WHEN checking for missing substring
        # THEN AssertionError with preview of actual text
        with pytest.raises(
            AssertionError,
            match="Expected result text to contain 'Goodbye'",
        ):
            assert_result_completed(result, contains="Goodbye")

    def test_fails_when_status_key_missing(self) -> None:
        # GIVEN a result dict with no "status" key at all
        # WHEN assert_result_completed is called
        # THEN it names the missing key instead of reading None as a status
        with pytest.raises(AssertionError, match="'status' key is missing"):
            assert_result_completed({"output": []})


class TestAssertResultFailed:
    """Tests for assert_result_failed."""

    def test_passes_on_failed_result(self) -> None:
        # GIVEN a failed result
        result = _failed_result("TIMEOUT")
        # WHEN / THEN the assertion passes without naming a code
        assert_result_failed(result)

    def test_passes_on_matching_error_code(self) -> None:
        # GIVEN a failed result carrying the TIMEOUT code
        result = _failed_result("TIMEOUT")
        # WHEN / THEN asserting that exact code passes
        assert_result_failed(result, code="TIMEOUT")

    def test_fails_on_wrong_error_code(self) -> None:
        # GIVEN a failed result carrying the TIMEOUT code
        result = _failed_result("TIMEOUT")
        # WHEN a different code is asserted
        # THEN the message names both the expected and the actual code
        with pytest.raises(
            AssertionError,
            match="Expected error code 'TOOL_ERROR', got 'TIMEOUT'",
        ):
            assert_result_failed(result, code="TOOL_ERROR")

    def test_fails_on_completed_status(self) -> None:
        # GIVEN a completed result
        result = _completed_result()
        # WHEN a failure is asserted
        # THEN the message names both statuses
        with pytest.raises(
            AssertionError,
            match="Expected result status 'failed', got 'completed'",
        ):
            assert_result_failed(result)


class TestAssertResultInputRequired:
    """Tests for assert_result_input_required."""

    def test_passes_on_input_required(self) -> None:
        # GIVEN a result asking for human input
        result = _input_required_result()
        # WHEN / THEN the assertion passes
        assert_result_input_required(result)

    def test_fails_on_completed(self) -> None:
        # GIVEN a completed result
        result = _completed_result()
        # WHEN input_required is asserted
        # THEN the message names both statuses
        with pytest.raises(
            AssertionError,
            match="Expected result status 'input_required', got 'completed'",
        ):
            assert_result_input_required(result)


class TestAssertToolCalled:
    """Tests for assert_tool_called (operating on the new MockContext)."""

    def test_passes_when_tool_was_called(self) -> None:
        # GIVEN a ctx with 3 "bash" calls
        ctx = MockContext()
        ctx.tools.calls.extend([("bash", {}), ("bash", {}), ("bash", {})])
        # WHEN / THEN no exception
        assert_tool_called(ctx, "bash")

    def test_passes_with_exact_count(self) -> None:
        # GIVEN a ctx with 3 "bash" calls
        ctx = MockContext()
        ctx.tools.calls.extend([("bash", {}), ("bash", {}), ("bash", {})])
        # WHEN / THEN asserting exactly 3 calls passes
        assert_tool_called(ctx, "bash", times=3)

    def test_fails_with_wrong_count(self) -> None:
        # GIVEN 3 calls
        ctx = MockContext()
        ctx.tools.calls.extend([("bash", {}), ("bash", {}), ("bash", {})])
        # WHEN checking for 2 calls
        # THEN AssertionError with explicit count message
        with pytest.raises(
            AssertionError,
            match="Expected tool 'bash' to be called 2 times, was called 3 times",
        ):
            assert_tool_called(ctx, "bash", times=2)

    def test_fails_when_tool_never_called(self) -> None:
        # GIVEN a ctx that recorded no tool call
        ctx = MockContext()
        # WHEN a call to "bash" is asserted
        # THEN the message says it was never called, not that the count differs
        with pytest.raises(
            AssertionError,
            match="Expected tool 'bash' to be called, but it was never called",
        ):
            assert_tool_called(ctx, "bash")

    def test_counts_only_matching_tool(self) -> None:
        # GIVEN a ctx with two "bash" calls and one "file_io" call
        ctx = MockContext()
        ctx.tools.calls.extend([("bash", {}), ("file_io", {}), ("bash", {})])
        # WHEN / THEN each tool is counted on its own name
        assert_tool_called(ctx, "bash", times=2)
        assert_tool_called(ctx, "file_io", times=1)


class TestAssertLlmCalled:
    """Tests for assert_llm_called."""

    def test_passes_when_llm_called(self) -> None:
        # GIVEN a ctx whose LLM was called once
        ctx = MockContext()
        ctx.llm.call_count = 1
        # WHEN / THEN the assertion passes without naming a count
        assert_llm_called(ctx)

    def test_passes_with_exact_count(self) -> None:
        # GIVEN a ctx whose LLM was called three times
        ctx = MockContext()
        ctx.llm.call_count = 3
        # WHEN / THEN asserting exactly 3 calls passes
        assert_llm_called(ctx, times=3)

    def test_fails_when_never_called(self) -> None:
        # GIVEN a ctx whose LLM was never called
        ctx = MockContext()
        # WHEN a call is asserted
        # THEN the message says it was never called
        with pytest.raises(
            AssertionError,
            match="Expected LLM to be called, but it was never called",
        ):
            assert_llm_called(ctx)

    def test_fails_with_wrong_count(self) -> None:
        # GIVEN a ctx whose LLM was called twice
        ctx = MockContext()
        ctx.llm.call_count = 2
        # WHEN five calls are asserted
        # THEN the message names both the expected and the actual count
        with pytest.raises(
            AssertionError,
            match="Expected LLM to be called 5 times, was called 2 times",
        ):
            assert_llm_called(ctx, times=5)

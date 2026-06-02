"""Assertion helpers for Apollia agent test suites.

Synchronous functions that raise :class:`AssertionError` on failure -
directly compatible with ``pytest``.  They inspect both the
:class:`AIPResult` dicts that agents return and the
:class:`~apollia.testing.MockContext` services to verify behaviour.

Example::

    from apollia.testing import (
        mock,
        assert_result_completed,
        assert_skill_called,
        assert_tool_called,
    )

    agent, ctx = mock(MyAgent)
    result = await agent.invoke_skill("my.skill", x=1)
    assert_result_completed(result, contains="done")
    assert_tool_called(ctx, "bash", times=1)
    assert_skill_called(ctx, "child.worker")
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from apollia.utils.formatting import aip_result_text

if TYPE_CHECKING:
    from apollia.testing.mock_factory import MockContext

_TEXT_PREVIEW_LIMIT = 100


# ──────────────────────────────────────────────────────────────────────
# AIPResult assertions
# ──────────────────────────────────────────────────────────────────────


def assert_result_completed(
    result: dict[str, Any],
    *,
    contains: str | None = None,
) -> None:
    """Verify an :class:`AIPResult` dict has ``status == "completed"``.

    Args:
        result: AIPResult dictionary to verify.
        contains: If provided, additionally check that the concatenated
            text parts of the output contain this substring.

    Raises:
        AssertionError: If the status is not ``completed`` or the text
            does not contain the expected substring.
    """
    _assert_status(result, "completed")

    if contains is not None:
        text = aip_result_text(result)
        if contains not in text:
            preview = text[:_TEXT_PREVIEW_LIMIT]
            raise AssertionError(f"Expected result text to contain '{contains}', got: '{preview}'")


def assert_result_failed(
    result: dict[str, Any],
    *,
    code: str | None = None,
) -> None:
    """Verify an :class:`AIPResult` dict has ``status == "failed"``.

    Args:
        result: AIPResult dictionary to verify.
        code: If provided, additionally check that ``error.code``
            matches exactly.

    Raises:
        AssertionError: If the status is not ``failed`` or the error
            code does not match.
    """
    _assert_status(result, "failed")

    if code is not None:
        error = result.get("error") or {}
        actual_code = error.get("code", "")
        if actual_code != code:
            raise AssertionError(f"Expected error code '{code}', got '{actual_code}'")


def assert_result_input_required(result: dict[str, Any]) -> None:
    """Verify an :class:`AIPResult` dict has ``status == "input_required"``.

    Raises:
        AssertionError: If the status is not ``input_required``.
    """
    _assert_status(result, "input_required")


# ──────────────────────────────────────────────────────────────────────
# MockContext interaction assertions
# ──────────────────────────────────────────────────────────────────────


def assert_skill_called(
    ctx: MockContext,
    skill_id: str,
    *,
    times: int | None = None,
) -> None:
    """Verify ``ctx.a2a.invoke(skill_id, ...)`` was called.

    Args:
        ctx: :class:`MockContext` instance returned by :func:`mock`.
        skill_id: The fully-qualified skill id (e.g. ``"pdf.read_text"``).
        times: If provided, assert the exact number of invocations.

    Raises:
        AssertionError: If the skill was never called (or not the
            expected number of times when ``times`` is set).
    """
    calls = [c for c in ctx.a2a.invoke_calls if c[0] == skill_id]
    if times is not None:
        if len(calls) != times:
            raise AssertionError(
                f"Expected skill '{skill_id}' to be invoked {times} times, "
                f"was invoked {len(calls)} times"
            )
    elif not calls:
        all_called = [c[0] for c in ctx.a2a.invoke_calls]
        raise AssertionError(
            f"Skill '{skill_id}' was not invoked via ctx.a2a.invoke "
            f"(invoked skills: {all_called})"
        )


def assert_tool_called(
    ctx: MockContext,
    tool_name: str,
    *,
    times: int | None = None,
) -> None:
    """Verify ``ctx.tools.call(tool_name, ...)`` was called.

    Args:
        ctx: :class:`MockContext` instance.
        tool_name: Name of the tool, e.g. ``"bash"`` or ``"file_io"``.
        times: If provided, assert the exact number of calls.

    Raises:
        AssertionError: If the tool was never called or the count differs.
    """
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
    *,
    times: int | None = None,
) -> None:
    """Verify that the LLM proxy was called.

    Args:
        ctx: :class:`MockContext` instance.
        times: If provided, assert the exact number of calls.

    Raises:
        AssertionError: If the LLM was never called or the count differs.
    """
    actual_count = ctx.llm.call_count

    if times is None:
        if actual_count == 0:
            raise AssertionError("Expected LLM to be called, but it was never called")
    elif actual_count != times:
        raise AssertionError(
            f"Expected LLM to be called {times} times, " f"was called {actual_count} times"
        )


def assert_emitted_token(
    ctx: MockContext,
    *,
    contains: str | None = None,
) -> None:
    """Verify that at least one token was emitted via ``ctx.events.emit_token``.

    Args:
        ctx: :class:`MockContext` instance.
        contains: If provided, assert that the concatenation of emitted
            tokens contains this substring.

    Raises:
        AssertionError: If no token was emitted, or the concatenation
            does not contain ``contains``.
    """
    if not ctx.events.tokens:
        raise AssertionError("No tokens were emitted via ctx.events.emit_token")
    if contains is not None:
        joined = "".join(ctx.events.tokens)
        if contains not in joined:
            raise AssertionError(f"Emitted tokens {joined!r} do not contain {contains!r}")


def assert_emitted_thought(
    ctx: MockContext,
    *,
    contains: str | None = None,
) -> None:
    """Verify that at least one thought was emitted via ``ctx.events.emit_thought``.

    Args:
        ctx: :class:`MockContext` instance.
        contains: If provided, assert that the concatenation of thought
            texts contains this substring.

    Raises:
        AssertionError: If no thought was emitted, or the concatenation
            does not contain ``contains``.
    """
    if not ctx.events.thoughts:
        raise AssertionError("No thoughts were emitted via ctx.events.emit_thought")
    if contains is not None:
        joined = " ".join(t[0] for t in ctx.events.thoughts)
        if contains not in joined:
            raise AssertionError(f"Emitted thoughts {joined!r} do not contain {contains!r}")


def assert_memory_recorded(
    ctx: MockContext,
    *,
    key: str | None = None,
) -> None:
    """Verify that the agent wrote to memory.

    Args:
        ctx: :class:`MockContext` instance.
        key: If provided, assert that this semantic key was remembered
            via ``ctx.memory.remember(key, ...)``.  If ``None``, assert
            that at least one episodic record was written.

    Raises:
        AssertionError: If the expected write did not happen.
    """
    if key is not None:
        if key not in ctx.memory.store:
            recorded = list(ctx.memory.store.keys())
            raise AssertionError(
                f"Memory key {key!r} was not remembered (recorded keys: {recorded})"
            )
    else:
        if not ctx.memory.episodes:
            raise AssertionError("No episodic memory was recorded via ctx.memory.record")


def assert_template_rendered(
    ctx: MockContext,
    name: str,
) -> None:
    """Verify ``ctx.templates.render(name, ...)`` was called.

    Args:
        ctx: :class:`MockContext` instance.
        name: Template name (e.g. ``"prompt.j2"``).

    Raises:
        AssertionError: If the template was never rendered.
    """
    names = [c[0] for c in ctx.templates.render_calls]
    if name not in names:
        raise AssertionError(f"Template {name!r} was not rendered (rendered: {names})")


# ──────────────────────────────────────────────────────────────────────
# Internal helpers
# ──────────────────────────────────────────────────────────────────────


def _assert_status(result: dict[str, Any], expected: str) -> None:
    """Verify the ``status`` field of an AIPResult dictionary.

    Raises:
        AssertionError: If the key is missing or the value differs.
    """
    actual = result.get("status")
    if actual is None:
        raise AssertionError(f"Expected result status '{expected}', but 'status' key is missing")
    if actual != expected:
        raise AssertionError(f"Expected result status '{expected}', got '{actual}'")


__all__ = [
    "assert_result_completed",
    "assert_result_failed",
    "assert_result_input_required",
    "assert_skill_called",
    "assert_tool_called",
    "assert_llm_called",
    "assert_emitted_token",
    "assert_emitted_thought",
    "assert_memory_recorded",
    "assert_template_rendered",
]

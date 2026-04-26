"""Tests for apollia.stubs — type stubs for PyO3 runtime objects."""

from apollia.stubs.context import RuntimeContext, StepBudgetView
from apollia.stubs.llm import LlmProxy, LlmResponse, TokenUsage
from apollia.stubs.memory import MemoryInterface
from apollia.stubs.tools import ToolProxy


def test_import_runtime_context():
    """RuntimeContext stub is importable."""
    assert RuntimeContext is not None


def test_import_tool_proxy():
    """ToolProxy stub is importable."""
    assert ToolProxy is not None


def test_import_llm_proxy():
    """LlmProxy stub is importable."""
    assert LlmProxy is not None


def test_import_llm_response():
    """LlmResponse stub is importable."""
    assert LlmResponse is not None


def test_import_token_usage():
    """TokenUsage stub is importable."""
    assert TokenUsage is not None


def test_import_memory_interface():
    """MemoryInterface stub is importable."""
    assert MemoryInterface is not None


def test_import_all_from_stubs():
    """All stubs importable from apollia.stubs."""
    from apollia.stubs import (
        LlmProxy,
        LlmResponse,
        MemoryInterface,
        RuntimeContext,
        TokenUsage,
        ToolProxy,
    )

    assert all(
        cls is not None
        for cls in [RuntimeContext, ToolProxy, LlmProxy, LlmResponse, TokenUsage, MemoryInterface]
    )


def test_tool_proxy_has_expected_methods():
    """ToolProxy exposes call, list_tools, tool_call_count, describe."""
    expected = {"call", "list_tools", "tool_call_count", "describe"}
    actual = {name for name in dir(ToolProxy) if not name.startswith("_")}
    assert expected.issubset(actual), f"Missing: {expected - actual}"


def test_llm_proxy_has_expected_methods():
    """LlmProxy exposes default_backend, chat, complete, stream, run_tools."""
    expected = {"default_backend", "chat", "complete", "stream", "run_tools"}
    actual = {name for name in dir(LlmProxy) if not name.startswith("_")}
    assert expected.issubset(actual), f"Missing: {expected - actual}"


def test_memory_interface_has_expected_methods():
    """MemoryInterface exposes record, remember, recall, recall_procedure, search, forget."""
    expected = {"record", "remember", "recall", "recall_procedure", "search", "forget"}
    actual = {name for name in dir(MemoryInterface) if not name.startswith("_")}
    assert expected.issubset(actual), f"Missing: {expected - actual}"


def test_memory_interface_has_recall_entry_and_recall_all():
    """MemoryInterface exposes recall_entry and recall_all."""
    expected = {"recall_entry", "recall_all"}
    actual = {name for name in dir(MemoryInterface) if not name.startswith("_")}
    assert expected.issubset(actual), f"Missing: {expected - actual}"


def test_runtime_context_has_expected_attrs():
    """RuntimeContext exposes tools, llm, memory, send, receive."""
    expected = {"tools", "llm", "memory", "send", "receive"}
    actual = {name for name in dir(RuntimeContext) if not name.startswith("_")}
    assert expected.issubset(actual), f"Missing: {expected - actual}"


def test_runtime_context_has_log_and_step_budget():
    """RuntimeContext exposes log() and step_budget."""
    expected = {"log", "step_budget"}
    actual = {name for name in dir(RuntimeContext) if not name.startswith("_")}
    assert expected.issubset(actual), f"Missing: {expected - actual}"


def test_step_budget_view_has_expected_attrs():
    """StepBudgetView has steps_remaining, tool_calls_remaining, elapsed_seconds."""
    expected = {"steps_remaining", "tool_calls_remaining", "elapsed_seconds"}
    actual = {name for name in dir(StepBudgetView) if not name.startswith("_")}
    assert expected.issubset(actual), f"Missing: {expected - actual}"


def test_step_budget_view_importable_from_stubs():
    """StepBudgetView is importable from apollia.stubs."""
    from apollia.stubs import StepBudgetView as SBV

    assert SBV is not None

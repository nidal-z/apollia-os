"""Tests for apollia.stubs — type stubs for PyO3 runtime objects."""

from apollia.stubs.context import RuntimeContext
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
    """MemoryInterface exposes record, remember, recall, search, forget."""
    expected = {"record", "remember", "recall", "search", "forget"}
    actual = {name for name in dir(MemoryInterface) if not name.startswith("_")}
    assert expected.issubset(actual), f"Missing: {expected - actual}"


def test_runtime_context_has_expected_attrs():
    """RuntimeContext exposes tools, llm, memory, send, receive."""
    expected = {"tools", "llm", "memory", "send", "receive"}
    actual = {name for name in dir(RuntimeContext) if not name.startswith("_")}
    assert expected.issubset(actual), f"Missing: {expected - actual}"

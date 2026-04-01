"""Tests for apollia.agents.worker — WorkerAgent helpers."""

from __future__ import annotations

from typing import Any
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from apollia.agents.worker import WorkerAgent
from apollia.agents.react import AIPResult


# ---------------------------------------------------------------------------
# Concrete subclass for testing
# ---------------------------------------------------------------------------


class ConcreteWorker(WorkerAgent):
    """Minimal concrete subclass for unit testing."""

    SYSTEM_PROMPT = "You are a test worker."

    def manifest(self) -> dict[str, Any]:
        return {
            "name": "test-worker",
            "version": "0.1.0",
            "supports_a2a": True,
            "skills": [{"id": "test-skill", "name": "Test Skill"}],
        }

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        return AIPResult.completed("done")


# ---------------------------------------------------------------------------
# Instantiation and inheritance
# ---------------------------------------------------------------------------


def test_worker_agent_is_abstract() -> None:
    """WorkerAgent cannot be instantiated directly."""
    with pytest.raises(TypeError):
        WorkerAgent()  # type: ignore[abstract]


def test_concrete_worker_instantiation() -> None:
    """Concrete subclass can be instantiated."""
    agent = ConcreteWorker()
    assert agent.manifest()["name"] == "test-worker"
    assert agent.manifest()["supports_a2a"] is True


@pytest.mark.asyncio
async def test_concrete_worker_run() -> None:
    """Concrete subclass run() returns an AIPResult dict."""
    agent = ConcreteWorker()
    result = await agent.run({}, None)
    assert result["status"] == "completed"


# ---------------------------------------------------------------------------
# delegate_skill
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_delegate_skill_delegates_to_ctx() -> None:
    """delegate_skill() calls ctx.delegate with correct arguments."""
    agent = ConcreteWorker()
    ctx = MagicMock()
    ctx.delegate = AsyncMock(
        return_value={"task_id": "t-1", "agent_name": "worker", "output": "ok"}
    )

    result = await agent.delegate_skill(ctx, "read-excel", {"path": "/tmp/f.xlsx"})

    ctx.delegate.assert_awaited_once_with("read-excel", {"path": "/tmp/f.xlsx"}, 120)
    assert result["output"] == "ok"


@pytest.mark.asyncio
async def test_delegate_skill_passes_custom_timeout() -> None:
    """delegate_skill() forwards timeout_secs to ctx.delegate."""
    agent = ConcreteWorker()
    ctx = MagicMock()
    ctx.delegate = AsyncMock(return_value={"task_id": "t-2", "agent_name": "w", "output": "x"})

    await agent.delegate_skill(ctx, "analyze-csv", {}, timeout_secs=30)

    ctx.delegate.assert_awaited_once_with("analyze-csv", {}, 30)


@pytest.mark.asyncio
async def test_delegate_skill_propagates_runtime_error() -> None:
    """delegate_skill() propagates RuntimeError from ctx.delegate."""
    agent = ConcreteWorker()
    ctx = MagicMock()
    ctx.delegate = AsyncMock(side_effect=RuntimeError("skill not found"))

    with pytest.raises(RuntimeError, match="skill not found"):
        await agent.delegate_skill(ctx, "unknown-skill", {})


# ---------------------------------------------------------------------------
# domain_error
# ---------------------------------------------------------------------------


def test_domain_error_returns_failed_dict() -> None:
    """domain_error() returns an AIPResult.failed() dict."""
    agent = ConcreteWorker()
    result = agent.domain_error("file_not_found", "File missing")
    assert result["status"] == "failed"
    assert result["error"]["code"] == "file_not_found"
    assert result["error"]["message"] == "File missing"


def test_domain_error_with_details() -> None:
    """domain_error() passes details through to the error dict."""
    agent = ConcreteWorker()
    result = agent.domain_error("parse_error", "Bad CSV", details={"line": 42})
    assert result["error"]["details"] == {"line": 42}


def test_domain_error_without_details() -> None:
    """domain_error() without details produces no details key."""
    agent = ConcreteWorker()
    result = agent.domain_error("encoding_error", "Bad encoding")
    assert "details" not in result["error"]


# ---------------------------------------------------------------------------
# check_python_result
# ---------------------------------------------------------------------------


def test_check_python_result_success_returns_stdout() -> None:
    """check_python_result() returns stdout on exit_code == 0."""
    agent = ConcreteWorker()
    result = agent.check_python_result(
        {"exit_code": 0, "stdout": "42\n", "stderr": ""},
        "compute",
    )
    assert result == "42\n"


def test_check_python_result_failure_returns_error_dict() -> None:
    """check_python_result() returns a failed dict on non-zero exit_code."""
    agent = ConcreteWorker()
    result = agent.check_python_result(
        {"exit_code": 1, "stdout": "", "stderr": "ZeroDivisionError"},
        "divide",
    )
    assert isinstance(result, dict)
    assert result["status"] == "failed"
    assert result["error"]["code"] == "python_execution_failed"
    assert "divide" in result["error"]["message"]


def test_check_python_result_missing_keys_defaults() -> None:
    """check_python_result() handles missing keys with sane defaults."""
    agent = ConcreteWorker()
    result = agent.check_python_result({}, "op")
    assert isinstance(result, dict)
    assert result["status"] == "failed"


# ---------------------------------------------------------------------------
# run_python, read_file, write_file, list_files — ctx.tools.call delegation
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_python_calls_python_executor() -> None:
    """run_python() calls ctx.tools.call('python_executor', ...)."""
    agent = ConcreteWorker()
    ctx = MagicMock()
    ctx.tools = MagicMock()
    ctx.tools.call = AsyncMock(return_value={"exit_code": 0, "stdout": "ok", "stderr": ""})

    result = await agent.run_python(ctx, "print('ok')", timeout_secs=10)

    ctx.tools.call.assert_awaited_once_with(
        "python_executor", {"code": "print('ok')", "timeout_secs": 10}
    )
    assert result["exit_code"] == 0


@pytest.mark.asyncio
async def test_read_file_returns_content() -> None:
    """read_file() returns the 'content' field from file_read."""
    agent = ConcreteWorker()
    ctx = MagicMock()
    ctx.tools = MagicMock()
    ctx.tools.call = AsyncMock(return_value={"content": "hello"})

    text = await agent.read_file(ctx, "/tmp/f.txt")

    ctx.tools.call.assert_awaited_once_with("file_read", {"path": "/tmp/f.txt"})
    assert text == "hello"


@pytest.mark.asyncio
async def test_read_file_missing_content_returns_empty() -> None:
    """read_file() returns '' when 'content' key is absent."""
    agent = ConcreteWorker()
    ctx = MagicMock()
    ctx.tools = MagicMock()
    ctx.tools.call = AsyncMock(return_value={})

    text = await agent.read_file(ctx, "/tmp/missing.txt")
    assert text == ""


@pytest.mark.asyncio
async def test_write_file_calls_file_write() -> None:
    """write_file() calls ctx.tools.call('file_write', ...)."""
    agent = ConcreteWorker()
    ctx = MagicMock()
    ctx.tools = MagicMock()
    ctx.tools.call = AsyncMock(return_value={})

    await agent.write_file(ctx, "/tmp/out.txt", "content here")

    ctx.tools.call.assert_awaited_once_with(
        "file_write", {"path": "/tmp/out.txt", "content": "content here"}
    )


@pytest.mark.asyncio
async def test_list_files_returns_entries() -> None:
    """list_files() returns the 'entries' list from file_list."""
    agent = ConcreteWorker()
    ctx = MagicMock()
    ctx.tools = MagicMock()
    ctx.tools.call = AsyncMock(return_value={"entries": ["a.py", "b.py"]})

    files = await agent.list_files(ctx, "/tmp/dir")

    ctx.tools.call.assert_awaited_once_with(
        "file_list", {"path": "/tmp/dir", "recursive": False}
    )
    assert files == ["a.py", "b.py"]


@pytest.mark.asyncio
async def test_list_files_recursive_flag() -> None:
    """list_files() passes recursive=True to file_list."""
    agent = ConcreteWorker()
    ctx = MagicMock()
    ctx.tools = MagicMock()
    ctx.tools.call = AsyncMock(return_value={"entries": []})

    await agent.list_files(ctx, "/tmp/dir", recursive=True)

    ctx.tools.call.assert_awaited_once_with(
        "file_list", {"path": "/tmp/dir", "recursive": True}
    )

"""Tests for WorkerAgent base class."""
import sys

import pytest

sys.path.insert(0, "sdk")

from apollia.agents import AIPResult, WorkerAgent
from apollia.agents.react import BaseReActAgent
from apollia.testing import MockContext


class ConcreteWorker(WorkerAgent):
    """Minimal concrete implementation used across all tests."""

    def manifest(self) -> dict:
        return {
            "name": "test-worker",
            "version": "0.1.0",
            "description": "",
            "tools_required": [],
        }

    async def run(self, task, ctx):
        return AIPResult.completed("ok")


# ── Import & inheritance ───────────────────────────────────────────────────────

def test_import_from_apollia_agents() -> None:
    # GIVEN the package is installed
    # WHEN importing WorkerAgent from the public API
    # THEN no ImportError is raised and the symbol resolves
    from apollia.agents import WorkerAgent as WA  # noqa: PLC0415

    assert WA is not None


def test_worker_agent_inherits_base_react_agent() -> None:
    # GIVEN WorkerAgent class
    # WHEN inspecting its MRO
    # THEN BaseReActAgent is in the chain
    assert issubclass(WorkerAgent, BaseReActAgent)
    assert BaseReActAgent in WorkerAgent.__mro__


# ── run_python ─────────────────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_run_python_calls_tool_and_returns_result() -> None:
    # GIVEN a worker and a context with python_executor configured
    worker = ConcreteWorker()
    ctx = MockContext.create(tools={
        "python_executor": {"stdout": "hello", "stderr": "", "exit_code": 0},
    })

    # WHEN run_python is called
    result = await worker.run_python(ctx, "print('hello')")

    # THEN the result contains the expected output
    assert result["stdout"] == "hello"
    assert result["exit_code"] == 0

    # AND python_executor was invoked exactly once
    assert ctx.tools is not None
    assert ctx.tools.tool_call_count() == 1
    tool_name, tool_args = ctx.tools.calls[0]
    assert tool_name == "python_executor"
    assert tool_args["code"] == "print('hello')"
    assert tool_args["timeout_secs"] == 30


@pytest.mark.asyncio
async def test_run_python_custom_timeout() -> None:
    # GIVEN a custom timeout
    worker = ConcreteWorker()
    ctx = MockContext.create(tools={
        "python_executor": {"stdout": "", "stderr": "", "exit_code": 0},
    })

    # WHEN run_python is called with timeout_secs=60
    await worker.run_python(ctx, "pass", timeout_secs=60)

    # THEN the timeout is forwarded
    assert ctx.tools is not None
    _, tool_args = ctx.tools.calls[0]
    assert tool_args["timeout_secs"] == 60


# ── check_python_result ────────────────────────────────────────────────────────

def test_check_python_result_success_returns_stdout() -> None:
    # GIVEN a successful result
    worker = ConcreteWorker()
    result = {"stdout": "résultat", "stderr": "", "exit_code": 0}

    # WHEN check_python_result is called
    output = worker.check_python_result(result, "lecture fichier")

    # THEN the stdout is returned as a string
    assert output == "résultat"


def test_check_python_result_failure_returns_error_dict() -> None:
    # GIVEN a failed execution result
    worker = ConcreteWorker()
    result = {"stdout": "", "stderr": "SyntaxError: invalid syntax", "exit_code": 1}

    # WHEN check_python_result is called
    output = worker.check_python_result(result, "lecture fichier")

    # THEN a typed failure dict is returned
    assert isinstance(output, dict)
    assert output["status"] == "failed"
    assert output["error"]["code"] == "python_execution_failed"
    assert "lecture fichier" in output["error"]["message"]
    assert "SyntaxError" in output["error"]["message"]


def test_check_python_result_failure_truncates_long_stderr() -> None:
    # GIVEN stderr longer than 500 chars
    worker = ConcreteWorker()
    long_stderr = "x" * 1000
    result = {"stdout": "", "stderr": long_stderr, "exit_code": 2}

    # WHEN check_python_result is called
    output = worker.check_python_result(result, "op")

    # THEN the message is truncated to 500 chars of stderr
    assert isinstance(output, dict)
    assert len(output["error"]["message"]) < len(long_stderr) + 50


def test_check_python_result_preserves_exit_code_in_details() -> None:
    # GIVEN a failed result with exit_code 127
    worker = ConcreteWorker()
    result = {"stdout": "", "stderr": "command not found", "exit_code": 127}

    # WHEN check_python_result is called
    output = worker.check_python_result(result, "exec")

    # THEN the exit_code is available in details
    assert isinstance(output, dict)
    assert output["error"]["details"]["exit_code"] == 127


# ── domain_error ───────────────────────────────────────────────────────────────

def test_domain_error_structure() -> None:
    # GIVEN a worker
    worker = ConcreteWorker()

    # WHEN domain_error is called with a code and message
    err = worker.domain_error("corrupted_file", "Le fichier est corrompu")

    # THEN the returned dict matches the AIPResult.failed() shape
    assert err["status"] == "failed"
    assert err["error"]["code"] == "corrupted_file"
    assert err["error"]["message"] == "Le fichier est corrompu"
    assert "details" not in err["error"]


def test_domain_error_with_details() -> None:
    # GIVEN extra details
    worker = ConcreteWorker()

    # WHEN domain_error is called with a details dict
    err = worker.domain_error("parse_error", "Bad CSV", {"line": 42})

    # THEN details are included in the error
    assert err["error"]["details"] == {"line": 42}


# ── file helpers ───────────────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_read_file_returns_content() -> None:
    # GIVEN file_read returns a content key
    worker = ConcreteWorker()
    ctx = MockContext.create(tools={
        "file_read": {"content": "file contents"},
    })

    # WHEN read_file is called
    content = await worker.read_file(ctx, "/tmp/test.txt")

    # THEN the content string is returned
    assert content == "file contents"


@pytest.mark.asyncio
async def test_read_file_missing_content_key_returns_empty() -> None:
    # GIVEN file_read returns a response without 'content' key
    worker = ConcreteWorker()
    ctx = MockContext.create(tools={"file_read": {}})

    # WHEN read_file is called
    content = await worker.read_file(ctx, "/tmp/empty.txt")

    # THEN an empty string is returned
    assert content == ""


@pytest.mark.asyncio
async def test_write_file_calls_tool() -> None:
    # GIVEN a write_file configured tool
    worker = ConcreteWorker()
    ctx = MockContext.create(tools={"file_write": {}})

    # WHEN write_file is called
    await worker.write_file(ctx, "/tmp/out.txt", "data")

    # THEN the tool was invoked with the expected args
    assert ctx.tools is not None
    assert ctx.tools.tool_call_count() == 1
    _, args = ctx.tools.calls[0]
    assert args["path"] == "/tmp/out.txt"
    assert args["content"] == "data"


@pytest.mark.asyncio
async def test_list_files_returns_entries() -> None:
    # GIVEN file_list returns a list of entries
    worker = ConcreteWorker()
    ctx = MockContext.create(tools={
        "file_list": {"entries": ["a.txt", "b.csv"]},
    })

    # WHEN list_files is called
    entries = await worker.list_files(ctx, "/tmp/dir")

    # THEN the list is returned
    assert entries == ["a.txt", "b.csv"]


@pytest.mark.asyncio
async def test_list_files_recursive_flag_forwarded() -> None:
    # GIVEN a list_files tool
    worker = ConcreteWorker()
    ctx = MockContext.create(tools={"file_list": {"entries": []}})

    # WHEN list_files is called with recursive=True
    await worker.list_files(ctx, "/tmp/dir", recursive=True)

    # THEN the recursive flag is forwarded
    assert ctx.tools is not None
    _, args = ctx.tools.calls[0]
    assert args["recursive"] is True

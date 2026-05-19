"""Tests for the task dispatch layer."""

from __future__ import annotations

import json
from typing import Any

import pytest

from apollia._internal.dispatch import (
    dispatch_message,
    dispatch_skill,
    dispatch_task,
    extract_task_message,
    extract_task_payload,
    extract_task_skill_id,
)
from apollia._internal.manifest import (
    ON_MESSAGE_HANDLER_ATTR,
    SKILLS_REGISTRY_ATTR,
    SkillEntry,
)
from apollia.errors import DomainError, NeedHumanInput


class _Ctx:
    """Minimal Ctx stand-in for tests."""

    logger = None


def _build_agent_with_skill(
    *,
    handler_name: str,
    handler: Any,
    skill_id: str,
    input_schema: dict[str, Any],
    requires_approval: bool = False,
    dangerous: bool = False,
) -> Any:
    """Build a tiny agent class+instance manually (no @skill decorator yet)."""

    class Agent:
        pass

    setattr(Agent, handler_name, handler)
    entry = SkillEntry(
        skill_id=skill_id,
        handler_name=handler_name,
        description="",
        input_schema=input_schema,
        output_schema={},
        requires_approval=requires_approval,
        dangerous=dangerous,
    )
    setattr(Agent, SKILLS_REGISTRY_ATTR, {skill_id: entry})
    return Agent()


# ────────────────────── extractors ──────────────────────


def test_extract_task_message_present() -> None:
    task = {"input": {"parts": [{"type": "text", "text": "hello"}]}}
    assert extract_task_message(task) == "hello"


def test_extract_task_message_missing() -> None:
    assert extract_task_message({}) == ""
    assert extract_task_message({"input": {}}) == ""
    assert extract_task_message({"input": {"parts": []}}) == ""


def test_extract_task_skill_id_present() -> None:
    assert extract_task_skill_id({"skill_id": "a.b"}) == "a.b"


def test_extract_task_skill_id_missing() -> None:
    assert extract_task_skill_id({}) is None
    assert extract_task_skill_id({"skill_id": ""}) is None
    assert extract_task_skill_id({"skill_id": None}) is None


def test_extract_payload_from_data_part() -> None:
    task = {
        "input": {
            "parts": [
                {"type": "text", "text": "hello"},
                {"type": "data", "data": {"path": "a.pdf"}},
            ]
        }
    }
    assert extract_task_payload(task) == {"path": "a.pdf"}


def test_extract_payload_from_text_part_json() -> None:
    task = {
        "input": {
            "parts": [
                {"type": "text", "text": json.dumps({"path": "a.pdf"})}
            ]
        }
    }
    assert extract_task_payload(task) == {"path": "a.pdf"}


def test_extract_payload_empty() -> None:
    assert extract_task_payload({"input": {"parts": []}}) == {}
    assert extract_task_payload({}) == {}
    # Non-JSON text → empty payload.
    assert extract_task_payload({"input": {"parts": [{"type": "text", "text": "raw"}]}}) == {}


# ────────────────────── dispatch_skill ──────────────────────


@pytest.mark.asyncio
async def test_dispatch_skill_happy_path() -> None:
    async def handler(self: Any, path: str, ctx: Any) -> str:  # noqa: ARG001
        return f"read:{path}"

    agent = _build_agent_with_skill(
        handler_name="handler",
        handler=handler,
        skill_id="parse",
        input_schema={
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
            "additionalProperties": False,
        },
    )
    result = await dispatch_skill(agent, "parse", {"path": "a.pdf"}, _Ctx())
    assert result["status"] == "completed"
    assert result["output"] == [{"type": "text", "text": "read:a.pdf"}]


@pytest.mark.asyncio
async def test_dispatch_skill_sync_handler() -> None:
    def handler(self: Any, x: int) -> dict[str, int]:  # noqa: ARG001
        return {"doubled": x * 2}

    agent = _build_agent_with_skill(
        handler_name="handler",
        handler=handler,
        skill_id="double",
        input_schema={
            "type": "object",
            "properties": {"x": {"type": "integer"}},
            "required": ["x"],
            "additionalProperties": False,
        },
    )
    result = await dispatch_skill(agent, "double", {"x": 21}, _Ctx())
    assert result["status"] == "completed"
    assert result["output"] == [{"type": "data", "data": {"doubled": 42}}]


@pytest.mark.asyncio
async def test_dispatch_skill_unknown() -> None:
    agent = _build_agent_with_skill(
        handler_name="h",
        handler=lambda self: None,
        skill_id="known",
        input_schema={"type": "object", "properties": {}, "additionalProperties": False},
    )
    result = await dispatch_skill(agent, "unknown", {}, _Ctx())
    assert result["status"] == "failed"
    assert result["error"]["code"] == "UNKNOWN_SKILL_ID"
    assert result["error"]["details"]["requested"] == "unknown"
    assert "known" in result["error"]["details"]["known"]


@pytest.mark.asyncio
async def test_dispatch_skill_payload_error() -> None:
    def handler(self: Any, path: str) -> str:  # noqa: ARG001
        return path

    agent = _build_agent_with_skill(
        handler_name="handler",
        handler=handler,
        skill_id="parse",
        input_schema={
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
            "additionalProperties": False,
        },
    )
    result = await dispatch_skill(agent, "parse", {}, _Ctx())
    assert result["status"] == "failed"
    assert result["error"]["code"] == "PAYLOAD_ERROR"
    assert result["error"]["details"]["field"] == "path"


@pytest.mark.asyncio
async def test_dispatch_skill_domain_error() -> None:
    def handler(self: Any, path: str) -> str:  # noqa: ARG001
        raise DomainError("FILE_NOT_FOUND", "missing", {"path": path})

    agent = _build_agent_with_skill(
        handler_name="handler",
        handler=handler,
        skill_id="read",
        input_schema={
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
            "additionalProperties": False,
        },
    )
    result = await dispatch_skill(agent, "read", {"path": "x"}, _Ctx())
    assert result["status"] == "failed"
    assert result["error"]["code"] == "FILE_NOT_FOUND"
    assert result["error"]["details"] == {"path": "x"}


@pytest.mark.asyncio
async def test_dispatch_skill_need_human_input() -> None:
    def handler(self: Any, path: str) -> str:  # noqa: ARG001
        raise NeedHumanInput("Approve?", {"path": path})

    agent = _build_agent_with_skill(
        handler_name="handler",
        handler=handler,
        skill_id="risky",
        input_schema={
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
            "additionalProperties": False,
        },
    )
    result = await dispatch_skill(agent, "risky", {"path": "x"}, _Ctx())
    assert result["status"] == "input_required"
    assert result["input_required_data"]["prompt"] == "Approve?"


@pytest.mark.asyncio
async def test_dispatch_skill_generic_exception() -> None:
    def handler(self: Any, path: str) -> str:  # noqa: ARG001
        raise ValueError("oops")

    agent = _build_agent_with_skill(
        handler_name="handler",
        handler=handler,
        skill_id="x",
        input_schema={
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
            "additionalProperties": False,
        },
    )
    result = await dispatch_skill(agent, "x", {"path": "x"}, _Ctx())
    assert result["status"] == "failed"
    assert result["error"]["code"] == "EXECUTION_FAILED"


@pytest.mark.asyncio
async def test_dispatch_skill_returns_dict() -> None:
    def handler(self: Any) -> dict[str, str]:  # noqa: ARG001
        return {"k": "v"}

    agent = _build_agent_with_skill(
        handler_name="handler",
        handler=handler,
        skill_id="x",
        input_schema={"type": "object", "properties": {}, "additionalProperties": False},
    )
    result = await dispatch_skill(agent, "x", {}, _Ctx())
    assert result["output"] == [{"type": "data", "data": {"k": "v"}}]


# ────────────────────── dispatch_message ──────────────────────


@pytest.mark.asyncio
async def test_dispatch_message_happy_path() -> None:
    class Agent:
        async def handle(self, message: str, history: list[dict[str, Any]], ctx: Any) -> str:
            assert isinstance(history, list)
            return f"echo:{message}"

    setattr(Agent, ON_MESSAGE_HANDLER_ATTR, "handle")
    agent = Agent()
    result = await dispatch_message(agent, "hello", None, _Ctx())
    assert result["status"] == "completed"
    assert result["output"] == [{"type": "text", "text": "echo:hello"}]


@pytest.mark.asyncio
async def test_dispatch_message_no_handler() -> None:
    class Agent:
        pass

    agent = Agent()
    result = await dispatch_message(agent, "hi", None, _Ctx())
    assert result["status"] == "failed"
    assert result["error"]["code"] == "NO_HANDLER"


@pytest.mark.asyncio
async def test_dispatch_message_traps_exception() -> None:
    class Agent:
        def handle(self, message: str, history: list[dict[str, Any]], ctx: Any) -> str:
            raise RuntimeError("boom")

    setattr(Agent, ON_MESSAGE_HANDLER_ATTR, "handle")
    agent = Agent()
    result = await dispatch_message(agent, "x", None, _Ctx())
    assert result["status"] == "failed"
    assert result["error"]["code"] == "EXECUTION_FAILED"


# ────────────────────── dispatch_task ──────────────────────


@pytest.mark.asyncio
async def test_dispatch_task_routes_skill() -> None:
    def handler(self: Any, path: str) -> str:  # noqa: ARG001
        return f"r:{path}"

    agent = _build_agent_with_skill(
        handler_name="handler",
        handler=handler,
        skill_id="parse",
        input_schema={
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
            "additionalProperties": False,
        },
    )
    task = {
        "skill_id": "parse",
        "input": {"parts": [{"type": "data", "data": {"path": "a"}}]},
    }
    result = await dispatch_task(agent, task, _Ctx())
    assert result["status"] == "completed"


@pytest.mark.asyncio
async def test_dispatch_task_routes_message() -> None:
    class Agent:
        def handle(self, message: str, history: list[dict[str, Any]], ctx: Any) -> str:
            return f"echo:{message}"

    setattr(Agent, ON_MESSAGE_HANDLER_ATTR, "handle")
    agent = Agent()
    task = {"input": {"parts": [{"type": "text", "text": "hi"}]}}
    result = await dispatch_task(agent, task, _Ctx())
    assert result["status"] == "completed"
    assert result["output"][0]["text"] == "echo:hi"


@pytest.mark.asyncio
async def test_dispatch_task_no_handler() -> None:
    class Agent:
        pass

    agent = Agent()
    task = {"input": {"parts": []}}
    result = await dispatch_task(agent, task, _Ctx())
    assert result["status"] == "failed"
    assert result["error"]["code"] == "NO_HANDLER"

"""Tests for the task dispatch layer."""

from __future__ import annotations

import json
from typing import Any

import pytest
from apollia._internal.dispatch import (
    _normalize_history,
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
    task = {"input": {"parts": [{"type": "text", "text": json.dumps({"path": "a.pdf"})}]}}
    assert extract_task_payload(task) == {"path": "a.pdf"}


def test_extract_payload_empty() -> None:
    assert extract_task_payload({"input": {"parts": []}}) == {}
    assert extract_task_payload({}) == {}
    # Non-JSON text → empty payload.
    assert extract_task_payload({"input": {"parts": [{"type": "text", "text": "raw"}]}}) == {}


# ────────────────────── dispatch_skill ──────────────────────


@pytest.mark.asyncio
async def test_dispatch_skill_happy_path() -> None:
    async def handler(self: Any, path: str, ctx: Any) -> str:  # NOSONAR
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
    def handler(self: Any, x: int) -> dict[str, int]:
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
    def handler(self: Any, path: str) -> str:
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
    def handler(self: Any, path: str) -> str:
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
    def handler(self: Any, path: str) -> str:
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
    def handler(self: Any, path: str) -> str:
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
    def handler(self: Any) -> dict[str, str]:
        return {"k": "v"}

    agent = _build_agent_with_skill(
        handler_name="handler",
        handler=handler,
        skill_id="x",
        input_schema={"type": "object", "properties": {}, "additionalProperties": False},
    )
    result = await dispatch_skill(agent, "x", {}, _Ctx())
    assert result["output"] == [{"type": "data", "data": {"k": "v"}}]


# ────────────────────── _normalize_history ──────────────────────


def test_normalize_history_flattens_aip_parts_to_content() -> None:
    # GIVEN AIP-shaped history (role + text parts), as the runtime serializes it
    raw = [
        {"role": "user", "parts": [{"type": "text", "text": "Bonjour"}]},
        {"role": "agent", "parts": [{"type": "text", "text": "Salut, ton prenom ?"}]},
    ]
    # WHEN normalizing for an @on_message handler
    history = _normalize_history(raw)
    # THEN each message is the SDK Message contract: role user/assistant + content
    assert history == [
        {"role": "user", "content": "Bonjour"},
        {"role": "assistant", "content": "Salut, ton prenom ?"},
    ]


def test_normalize_history_passes_through_content_and_guards_bad_input() -> None:
    # GIVEN an already-normalized message and non-list input
    assert _normalize_history([{"role": "assistant", "content": "hi"}]) == [
        {"role": "assistant", "content": "hi"}
    ]
    # THEN a non-list yields an empty history rather than raising
    assert _normalize_history(None) == []


# ────────────────────── dispatch_message ──────────────────────


@pytest.mark.asyncio
async def test_dispatch_message_happy_path() -> None:
    class Agent:
        async def handle(
            self, message: str, history: list[dict[str, Any]], ctx: Any
        ) -> str:  # NOSONAR
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
    def handler(self: Any, path: str) -> str:
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


# ────────────────────── enriched error surfaces ──────────────────────


@pytest.mark.asyncio
async def test_payload_error_lists_expected_fields_in_details() -> None:
    """The dispatch boundary preserves ``expected`` / ``unexpected`` from PayloadError details."""

    def handler(self: Any, path: str) -> str:
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
    result = await dispatch_skill(agent, "parse", {"path": "x", "bogus": 1}, _Ctx())
    assert result["status"] == "failed"
    assert result["error"]["code"] == "PAYLOAD_ERROR"
    details = result["error"]["details"]
    assert details["unexpected"] == "bogus"
    assert details["expected"] == ["path"]


@pytest.mark.asyncio
async def test_payload_error_did_you_mean_propagated() -> None:
    """A close typo surfaces a ``did_you_mean`` hint through the AIPResult details."""

    def handler(self: Any, path: str, mode: str = "fast") -> str:
        return path

    agent = _build_agent_with_skill(
        handler_name="handler",
        handler=handler,
        skill_id="parse",
        input_schema={
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "mode": {"type": "string"},
            },
            "required": ["path"],
            "additionalProperties": False,
        },
    )
    # ``path`` is supplied (required satisfied); ``mod`` is the typo that
    # should trigger the suggestion.
    result = await dispatch_skill(agent, "parse", {"path": "x", "mod": "fast"}, _Ctx())
    assert result["status"] == "failed"
    assert result["error"]["code"] == "PAYLOAD_ERROR"
    assert result["error"]["details"]["did_you_mean"] == "mode"


@pytest.mark.asyncio
async def test_skill_not_found_lists_known_skills() -> None:
    """A missing skill_id surfaces the list of available skills for steering the LLM."""

    def handler(self: Any) -> str:
        return "ok"

    agent = _build_agent_with_skill(
        handler_name="h",
        handler=handler,
        skill_id="chart.bar",
        input_schema={"type": "object", "properties": {}, "additionalProperties": False},
    )
    result = await dispatch_skill(agent, "chart.barr", {}, _Ctx())
    assert result["status"] == "failed"
    assert result["error"]["code"] == "UNKNOWN_SKILL_ID"
    assert result["error"]["details"]["requested"] == "chart.barr"
    assert result["error"]["details"]["known"] == ["chart.bar"]


@pytest.mark.asyncio
async def test_payload_error_type_mismatch_carries_typed_details() -> None:
    """A type mismatch surfaces ``expected_type``/``actual_type`` for LLM repair."""

    def handler(self: Any, count: int) -> str:
        return str(count)

    agent = _build_agent_with_skill(
        handler_name="handler",
        handler=handler,
        skill_id="count",
        input_schema={
            "type": "object",
            "properties": {"count": {"type": "integer"}},
            "required": ["count"],
            "additionalProperties": False,
        },
    )
    result = await dispatch_skill(agent, "count", {"count": "five"}, _Ctx())
    assert result["status"] == "failed"
    assert result["error"]["code"] == "PAYLOAD_ERROR"
    details = result["error"]["details"]
    assert details["expected_type"] == "integer"
    assert details["actual_type"] == "string"

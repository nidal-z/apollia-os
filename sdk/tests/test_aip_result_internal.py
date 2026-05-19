"""Tests for the internal AIPResult builders."""

from __future__ import annotations

import base64
import logging
from dataclasses import dataclass
from typing import NamedTuple

import pytest

from apollia._internal.aip_result import (
    completed,
    failed,
    from_exception,
    from_handler_return,
    input_required,
)
from apollia.errors import (
    AgentConfigError,
    DomainError,
    NeedHumanInput,
    PayloadError,
    SchemaError,
    SkillNotFound,
)


# ──────────────────────────── builders ────────────────────────────


def test_completed_with_text() -> None:
    result = completed("hello")
    assert result == {
        "task_id": "",
        "status": "completed",
        "output": [{"type": "text", "text": "hello"}],
        "error": None,
        "artifacts": [],
        "input_required_data": None,
    }


def test_completed_empty() -> None:
    result = completed()
    assert result["status"] == "completed"
    assert result["output"] == []
    assert result["error"] is None
    assert result["input_required_data"] is None


def test_completed_with_data_only() -> None:
    result = completed(data={"x": 1})
    assert result["output"] == [{"type": "data", "data": {"x": 1}}]


def test_completed_with_text_and_data() -> None:
    result = completed("hi", data={"k": 2})
    assert result["output"] == [
        {"type": "text", "text": "hi"},
        {"type": "data", "data": {"k": 2}},
    ]


def test_completed_with_artifacts() -> None:
    art = {"name": "a.txt", "mime_type": "text/plain", "data": b""}
    result = completed("ok", artifacts=[art])
    assert result["artifacts"] == [art]


def test_failed_basic() -> None:
    result = failed("BAD", "oops")
    assert result["status"] == "failed"
    assert result["error"] == {"code": "BAD", "message": "oops", "details": None}
    assert result["output"] == []
    assert result["input_required_data"] is None


def test_failed_with_details() -> None:
    result = failed("X", "y", {"k": "v"})
    assert result["error"]["details"] == {"k": "v"}


def test_input_required_basic() -> None:
    result = input_required("Approve?", {"id": 1})
    assert result["status"] == "input_required"
    assert result["input_required_data"] == {"prompt": "Approve?", "context": {"id": 1}}
    assert result["error"] is None
    assert result["output"] == []


def test_input_required_default_context() -> None:
    result = input_required("Continue?")
    assert result["input_required_data"]["context"] == {}


# ────────────────────── from_handler_return ──────────────────────


def test_from_handler_return_string() -> None:
    result = from_handler_return("hello")
    assert result["output"] == [{"type": "text", "text": "hello"}]


def test_from_handler_return_none() -> None:
    result = from_handler_return(None)
    assert result["status"] == "completed"
    assert result["output"] == []


def test_from_handler_return_dict() -> None:
    result = from_handler_return({"a": 1})
    assert result["output"] == [{"type": "data", "data": {"a": 1}}]


def test_from_handler_return_list() -> None:
    result = from_handler_return([1, 2, 3])
    assert result["output"] == [{"type": "data", "data": [1, 2, 3]}]


def test_from_handler_return_int() -> None:
    result = from_handler_return(42)
    assert result["output"] == [{"type": "text", "text": "42"}]


def test_from_handler_return_bytes() -> None:
    payload = b"\x01\x02\x03"
    result = from_handler_return(payload)
    expected = base64.b64encode(payload).decode("ascii")
    assert result["output"] == [{"type": "data", "data": {"__bytes__": expected}}]


def test_from_handler_return_dataclass() -> None:
    @dataclass
    class Foo:
        a: int
        b: str

    result = from_handler_return(Foo(a=1, b="x"))
    assert result["output"] == [{"type": "data", "data": {"a": 1, "b": "x"}}]


def test_from_handler_return_namedtuple() -> None:
    class P(NamedTuple):
        x: int
        y: int

    result = from_handler_return(P(1, 2))
    assert result["output"] == [{"type": "data", "data": {"x": 1, "y": 2}}]


def test_from_handler_return_plain_tuple() -> None:
    result = from_handler_return((1, 2, 3))
    assert result["output"] == [{"type": "data", "data": [1, 2, 3]}]


def test_from_handler_return_arbitrary_object() -> None:
    class Custom:
        def __str__(self) -> str:
            return "custom-repr"

    result = from_handler_return(Custom())
    assert result["output"] == [{"type": "text", "text": "custom-repr"}]


# ────────────────────── from_exception ──────────────────────


def test_from_exception_domain_error() -> None:
    result = from_exception(DomainError("X", "msg", {"k": "v"}))
    assert result["status"] == "failed"
    assert result["error"] == {"code": "X", "message": "msg", "details": {"k": "v"}}


def test_from_exception_need_human_input() -> None:
    result = from_exception(NeedHumanInput("Approve?", {"c": 1}))
    assert result["status"] == "input_required"
    assert result["input_required_data"] == {"prompt": "Approve?", "context": {"c": 1}}


def test_from_exception_payload_error_field() -> None:
    result = from_exception(PayloadError("missing", field="path"))
    assert result["error"]["code"] == "PAYLOAD_ERROR"
    assert result["error"]["message"] == "missing"
    assert result["error"]["details"] == {"field": "path"}


def test_from_exception_payload_error_with_extra_details() -> None:
    result = from_exception(
        PayloadError("bad", field="x", details={"hint": "see schema"})
    )
    assert result["error"]["details"] == {"field": "x", "hint": "see schema"}


def test_from_exception_payload_error_no_field() -> None:
    result = from_exception(PayloadError("bad"))
    assert result["error"]["code"] == "PAYLOAD_ERROR"
    assert result["error"]["details"] is None


def test_from_exception_schema_error() -> None:
    result = from_exception(SchemaError("unsupported"))
    assert result["error"]["code"] == "SCHEMA_ERROR"


def test_from_exception_skill_not_found() -> None:
    result = from_exception(SkillNotFound("a.b", known=["a.c"]))
    assert result["error"]["code"] == "UNKNOWN_SKILL_ID"
    assert result["error"]["details"] == {"requested": "a.b", "known": ["a.c"]}


def test_from_exception_agent_config_error() -> None:
    result = from_exception(AgentConfigError("bad config"))
    assert result["error"]["code"] == "AGENT_CONFIG_ERROR"


def test_from_exception_generic() -> None:
    result = from_exception(ValueError("oops"))
    assert result["error"]["code"] == "EXECUTION_FAILED"
    assert result["error"]["message"] == "oops"


def test_from_exception_generic_with_logger(
    caplog: pytest.LogCaptureFixture,
) -> None:
    logger = logging.getLogger("apollia.test")
    with caplog.at_level(logging.ERROR, logger="apollia.test"):
        result = from_exception(RuntimeError("boom"), logger=logger)
    assert result["error"]["code"] == "EXECUTION_FAILED"
    assert any("unhandled exception" in r.message for r in caplog.records)

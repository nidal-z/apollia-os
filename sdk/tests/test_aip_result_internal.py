"""Tests for the internal AIPResult builders."""

from __future__ import annotations

import base64
import logging
from dataclasses import dataclass
from typing import TYPE_CHECKING, NamedTuple

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

if TYPE_CHECKING:
    import pytest

# ──────────────────────────── builders ────────────────────────────


def test_completed_with_text() -> None:
    # GIVEN a successful outcome carrying one line of text
    # WHEN the completed() builder runs
    result = completed("hello")

    # THEN every field of the wire shape is set, error and input left empty
    assert result == {
        "task_id": "",
        "status": "completed",
        "output": [{"type": "text", "text": "hello"}],
        "error": None,
        "artifacts": [],
        "input_required_data": None,
    }


def test_completed_empty() -> None:
    # GIVEN a successful outcome carrying nothing
    # WHEN the completed() builder runs
    result = completed()

    # THEN the output is an empty list rather than a null or a blank text block
    assert result["status"] == "completed"
    assert result["output"] == []
    assert result["error"] is None
    assert result["input_required_data"] is None


def test_completed_with_data_only() -> None:
    # GIVEN a successful outcome carrying structured data and no text
    # WHEN the completed() builder runs
    result = completed(data={"x": 1})

    # THEN a single data block is emitted, with no empty text block beside it
    assert result["output"] == [{"type": "data", "data": {"x": 1}}]


def test_completed_with_text_and_data() -> None:
    # GIVEN a successful outcome carrying both text and data
    # WHEN the completed() builder runs
    result = completed("hi", data={"k": 2})

    # THEN both blocks are emitted, text first
    assert result["output"] == [
        {"type": "text", "text": "hi"},
        {"type": "data", "data": {"k": 2}},
    ]


def test_completed_with_artifacts() -> None:
    # GIVEN a successful outcome carrying one artifact
    art = {"name": "a.txt", "mime_type": "text/plain", "data": b""}

    # WHEN the completed() builder runs
    result = completed("ok", artifacts=[art])

    # THEN the artifact is carried through unchanged
    assert result["artifacts"] == [art]


def test_failed_basic() -> None:
    # GIVEN a failure with a code and a message
    # WHEN the failed() builder runs
    result = failed("BAD", "oops")

    # THEN the error block is filled and the output stays empty
    assert result["status"] == "failed"
    assert result["error"] == {"code": "BAD", "message": "oops", "details": None}
    assert result["output"] == []
    assert result["input_required_data"] is None


def test_failed_with_details() -> None:
    # GIVEN a failure carrying a details dict
    # WHEN the failed() builder runs
    result = failed("X", "y", {"k": "v"})

    # THEN the details are carried through unchanged
    assert result["error"]["details"] == {"k": "v"}


def test_input_required_basic() -> None:
    # GIVEN a pause asking the human a question, with context
    # WHEN the input_required() builder runs
    result = input_required("Approve?", {"id": 1})

    # THEN prompt and context are carried, and neither error nor output is set
    assert result["status"] == "input_required"
    assert result["input_required_data"] == {"prompt": "Approve?", "context": {"id": 1}}
    assert result["error"] is None
    assert result["output"] == []


def test_input_required_default_context() -> None:
    # GIVEN a pause with no context
    # WHEN the input_required() builder runs
    result = input_required("Continue?")

    # THEN the context is an empty dict, not None
    assert result["input_required_data"]["context"] == {}


# ────────────────────── from_handler_return ──────────────────────


def test_from_handler_return_string() -> None:
    # GIVEN a handler that returned a string
    # WHEN its return value is converted
    result = from_handler_return("hello")

    # THEN it becomes one text block
    assert result["output"] == [{"type": "text", "text": "hello"}]


def test_from_handler_return_none() -> None:
    # GIVEN a handler that returned nothing
    # WHEN its return value is converted
    result = from_handler_return(None)

    # THEN the task is completed with an empty output, not a "None" text block
    assert result["status"] == "completed"
    assert result["output"] == []


def test_from_handler_return_dict() -> None:
    # GIVEN a handler that returned a dict
    # WHEN its return value is converted
    result = from_handler_return({"a": 1})

    # THEN it becomes one data block, not a stringified dict
    assert result["output"] == [{"type": "data", "data": {"a": 1}}]


def test_from_handler_return_list() -> None:
    # GIVEN a handler that returned a list
    # WHEN its return value is converted
    result = from_handler_return([1, 2, 3])

    # THEN it becomes one data block carrying the list
    assert result["output"] == [{"type": "data", "data": [1, 2, 3]}]


def test_from_handler_return_int() -> None:
    # GIVEN a handler that returned an int
    # WHEN its return value is converted
    result = from_handler_return(42)

    # THEN it becomes a text block, because a scalar is not structured data
    assert result["output"] == [{"type": "text", "text": "42"}]


def test_from_handler_return_bytes() -> None:
    # GIVEN a handler that returned raw bytes
    payload = b"\x01\x02\x03"

    # WHEN its return value is converted
    result = from_handler_return(payload)

    # THEN the bytes are base64-encoded under the __bytes__ key, JSON-safe
    expected = base64.b64encode(payload).decode("ascii")
    assert result["output"] == [{"type": "data", "data": {"__bytes__": expected}}]


def test_from_handler_return_dataclass() -> None:
    # GIVEN a handler that returned a dataclass instance
    @dataclass
    class Foo:
        a: int
        b: str

    # WHEN its return value is converted
    result = from_handler_return(Foo(a=1, b="x"))

    # THEN the fields become a data block, not a repr string
    assert result["output"] == [{"type": "data", "data": {"a": 1, "b": "x"}}]


def test_from_handler_return_namedtuple() -> None:
    # GIVEN a handler that returned a NamedTuple
    class P(NamedTuple):
        x: int
        y: int

    # WHEN its return value is converted
    result = from_handler_return(P(1, 2))

    # THEN the field names survive, so it is a mapping and not a list
    assert result["output"] == [{"type": "data", "data": {"x": 1, "y": 2}}]


def test_from_handler_return_plain_tuple() -> None:
    # GIVEN a handler that returned a plain tuple, which carries no field names
    # WHEN its return value is converted
    result = from_handler_return((1, 2, 3))

    # THEN it becomes a list, unlike the NamedTuple case above
    assert result["output"] == [{"type": "data", "data": [1, 2, 3]}]


def test_from_handler_return_arbitrary_object() -> None:
    # GIVEN a handler that returned an object with a custom __str__
    class Custom:
        def __str__(self) -> str:
            return "custom-repr"

    # WHEN its return value is converted
    result = from_handler_return(Custom())

    # THEN the last-resort path renders it through str() into a text block
    assert result["output"] == [{"type": "text", "text": "custom-repr"}]


# ────────────────────── from_exception ──────────────────────


def test_from_exception_domain_error() -> None:
    # GIVEN a DomainError carrying a code, a message and details
    # WHEN it is converted to a result
    result = from_exception(DomainError("X", "msg", {"k": "v"}))

    # THEN the agent's own code reaches the wire untouched
    assert result["status"] == "failed"
    assert result["error"] == {"code": "X", "message": "msg", "details": {"k": "v"}}


def test_from_exception_need_human_input() -> None:
    # GIVEN a NeedHumanInput raised by the handler
    # WHEN it is converted to a result
    result = from_exception(NeedHumanInput("Approve?", {"c": 1}))

    # THEN it becomes a pause, not a failure
    assert result["status"] == "input_required"
    assert result["input_required_data"] == {"prompt": "Approve?", "context": {"c": 1}}


def test_from_exception_payload_error_field() -> None:
    # GIVEN a PayloadError naming the offending field
    # WHEN it is converted to a result
    result = from_exception(PayloadError("missing", field="path"))

    # THEN the field lands in the details under its own key
    assert result["error"]["code"] == "PAYLOAD_ERROR"
    assert result["error"]["message"] == "missing"
    assert result["error"]["details"] == {"field": "path"}


def test_from_exception_payload_error_with_extra_details() -> None:
    # GIVEN a PayloadError carrying both a field and extra details
    # WHEN it is converted to a result
    result = from_exception(PayloadError("bad", field="x", details={"hint": "see schema"}))

    # THEN the field is merged into the details instead of replacing them
    assert result["error"]["details"] == {"field": "x", "hint": "see schema"}


def test_from_exception_payload_error_no_field() -> None:
    # GIVEN a PayloadError naming no field
    # WHEN it is converted to a result
    result = from_exception(PayloadError("bad"))

    # THEN the details stay None rather than becoming an empty dict
    assert result["error"]["code"] == "PAYLOAD_ERROR"
    assert result["error"]["details"] is None


def test_from_exception_schema_error() -> None:
    # GIVEN a SchemaError
    # WHEN it is converted to a result
    result = from_exception(SchemaError("unsupported"))

    # THEN it maps to its own wire code
    assert result["error"]["code"] == "SCHEMA_ERROR"


def test_from_exception_skill_not_found() -> None:
    # GIVEN a SkillNotFound carrying the known skill list
    # WHEN it is converted to a result
    result = from_exception(SkillNotFound("a.b", known=["a.c"]))

    # THEN the caller gets both what was asked for and what exists
    assert result["error"]["code"] == "UNKNOWN_SKILL_ID"
    assert result["error"]["details"] == {"requested": "a.b", "known": ["a.c"]}


def test_from_exception_agent_config_error() -> None:
    # GIVEN an AgentConfigError
    # WHEN it is converted to a result
    result = from_exception(AgentConfigError("bad config"))

    # THEN it maps to its own wire code
    assert result["error"]["code"] == "AGENT_CONFIG_ERROR"


def test_from_exception_generic() -> None:
    # GIVEN an exception outside the SDK hierarchy
    # WHEN it is converted to a result
    result = from_exception(ValueError("oops"))

    # THEN it maps to the catch-all code, and its message survives
    assert result["error"]["code"] == "EXECUTION_FAILED"
    assert result["error"]["message"] == "oops"


def test_from_exception_generic_with_logger(
    caplog: pytest.LogCaptureFixture,
) -> None:
    # GIVEN a logger passed to the converter
    logger = logging.getLogger("apollia.test")

    # WHEN an exception outside the SDK hierarchy is converted
    with caplog.at_level(logging.ERROR, logger="apollia.test"):
        result = from_exception(RuntimeError("boom"), logger=logger)

    # THEN the failure is both returned and journalled, not swallowed
    assert result["error"]["code"] == "EXECUTION_FAILED"
    assert any("unhandled exception" in r.message for r in caplog.records)

"""Tests for the typed exception hierarchy."""

from __future__ import annotations

import pytest

from apollia.errors import (
    AgentConfigError,
    AgentError,
    DomainError,
    NeedHumanInput,
    PayloadError,
    SchemaError,
    SkillNotFound,
)


def test_agent_error_is_exception() -> None:
    assert issubclass(AgentError, Exception)


def test_all_inherit_from_agent_error() -> None:
    for cls in (
        DomainError,
        NeedHumanInput,
        PayloadError,
        SchemaError,
        SkillNotFound,
        AgentConfigError,
    ):
        assert issubclass(cls, AgentError)


def test_domain_error_minimal() -> None:
    err = DomainError("FILE_NOT_FOUND", "file is missing")
    assert err.code == "FILE_NOT_FOUND"
    assert err.message == "file is missing"
    assert err.details is None
    assert str(err) == "file is missing"


def test_domain_error_with_details() -> None:
    err = DomainError("X", "y", {"path": "/tmp/a"})
    assert err.details == {"path": "/tmp/a"}


def test_need_human_input_defaults_to_empty_context() -> None:
    err = NeedHumanInput("Approve?")
    assert err.prompt == "Approve?"
    assert err.context == {}
    assert str(err) == "Approve?"


def test_need_human_input_with_context() -> None:
    err = NeedHumanInput("Approve?", {"task_id": "1"})
    assert err.context == {"task_id": "1"}


def test_payload_error_default_field() -> None:
    err = PayloadError("bad payload")
    assert err.message == "bad payload"
    assert err.field is None
    assert err.details is None


def test_payload_error_with_field_and_details() -> None:
    err = PayloadError("missing", field="path", details={"hint": "see schema"})
    assert err.field == "path"
    assert err.details == {"hint": "see schema"}


def test_schema_error_str() -> None:
    err = SchemaError("unsupported type")
    assert str(err) == "unsupported type"


def test_skill_not_found_default_known() -> None:
    err = SkillNotFound("a.b")
    assert err.skill_id == "a.b"
    assert err.known == []
    assert "a.b" in str(err)


def test_skill_not_found_with_known() -> None:
    err = SkillNotFound("a.b", known=["a.c", "a.d"])
    assert err.known == ["a.c", "a.d"]


def test_agent_config_error_str() -> None:
    err = AgentConfigError("name must not be empty")
    assert str(err) == "name must not be empty"


def test_raise_and_catch() -> None:
    with pytest.raises(AgentError):
        raise DomainError("X", "y")
    with pytest.raises(DomainError):
        raise DomainError("X", "y")

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
    # GIVEN the root of the SDK error hierarchy
    # WHEN its ancestry is inspected
    # THEN it is a plain Exception subclass, catchable by generic handlers
    assert issubclass(AgentError, Exception)


def test_all_inherit_from_agent_error() -> None:
    # GIVEN every error type the SDK exposes
    for cls in (
        DomainError,
        NeedHumanInput,
        PayloadError,
        SchemaError,
        SkillNotFound,
        AgentConfigError,
    ):
        # WHEN its ancestry is inspected
        # THEN it is rooted at AgentError, so the dispatcher can map it
        assert issubclass(cls, AgentError)


def test_domain_error_minimal() -> None:
    # GIVEN a domain error built with a code and a message only
    err = DomainError("FILE_NOT_FOUND", "file is missing")

    # WHEN its fields are read
    # THEN details stays None and the message is what str() renders
    assert err.code == "FILE_NOT_FOUND"
    assert err.message == "file is missing"
    assert err.details is None
    assert str(err) == "file is missing"


def test_domain_error_with_details() -> None:
    # GIVEN a domain error built with a details dict
    err = DomainError("X", "y", {"path": "/srv/data/a"})

    # WHEN the details are read
    # THEN they are carried through unchanged
    assert err.details == {"path": "/srv/data/a"}


def test_need_human_input_defaults_to_empty_context() -> None:
    # GIVEN a human-input request built with a prompt only
    err = NeedHumanInput("Approve?")

    # WHEN its fields are read
    # THEN the context is an empty dict, not None, and str() renders the prompt
    assert err.prompt == "Approve?"
    assert err.context == {}
    assert str(err) == "Approve?"


def test_need_human_input_with_context() -> None:
    # GIVEN a human-input request built with a context dict
    err = NeedHumanInput("Approve?", {"task_id": "1"})

    # WHEN the context is read
    # THEN it is carried through unchanged
    assert err.context == {"task_id": "1"}


def test_payload_error_default_field() -> None:
    # GIVEN a payload error built with a message only
    err = PayloadError("bad payload")

    # WHEN its fields are read
    # THEN neither the offending field nor the details are invented
    assert err.message == "bad payload"
    assert err.field is None
    assert err.details is None


def test_payload_error_with_field_and_details() -> None:
    # GIVEN a payload error naming the offending field and carrying a hint
    err = PayloadError("missing", field="path", details={"hint": "see schema"})

    # WHEN its fields are read
    # THEN both are carried through unchanged
    assert err.field == "path"
    assert err.details == {"hint": "see schema"}


def test_schema_error_str() -> None:
    # GIVEN a schema error built with a message
    err = SchemaError("unsupported type")

    # WHEN it is rendered as text
    # THEN the message is what comes out, with no decoration
    assert str(err) == "unsupported type"


def test_skill_not_found_default_known() -> None:
    # GIVEN a lookup failure built without the list of known skills
    err = SkillNotFound("a.b")

    # WHEN its fields and text are read
    # THEN the known list is empty and the missing identifier is in the message
    assert err.skill_id == "a.b"
    assert err.known == []
    assert "a.b" in str(err)


def test_skill_not_found_with_known() -> None:
    # GIVEN a lookup failure built with the list of known skills
    err = SkillNotFound("a.b", known=["a.c", "a.d"])

    # WHEN the known list is read
    # THEN it is carried through unchanged, so the caller can suggest
    assert err.known == ["a.c", "a.d"]


def test_agent_config_error_str() -> None:
    # GIVEN a configuration error built with a message
    err = AgentConfigError("name must not be empty")

    # WHEN it is rendered as text
    # THEN the message is what comes out, with no decoration
    assert str(err) == "name must not be empty"


def test_raise_and_catch() -> None:
    # GIVEN a raised domain error
    # WHEN it is caught by the root type
    # THEN it is caught, and it is still caught by its own type
    with pytest.raises(AgentError):
        raise DomainError("X", "y")
    with pytest.raises(DomainError):
        raise DomainError("X", "y")

"""Tests for apollia.types - AIPResult dataclass and factory methods."""

from apollia.types import AIPResult


def test_aip_result_completed():
    """AIPResult.completed() returns status='completed' with text."""
    # GIVEN a successful outcome carrying only text
    # WHEN the completed() factory builds the result
    result = AIPResult.completed("done")

    # THEN status and text are set and data defaults to an empty dict
    assert result.status == "completed"
    assert result.text == "done"
    assert result.data == {}


def test_aip_result_completed_with_data():
    """AIPResult.completed() accepts optional data dict."""
    # GIVEN a successful outcome carrying structured data
    # WHEN the completed() factory builds the result
    result = AIPResult.completed("done", data={"key": "value"})

    # THEN the data dict is carried through unchanged
    assert result.status == "completed"
    assert result.data == {"key": "value"}


def test_aip_result_failed():
    """AIPResult.failed() returns status='failed' with error info."""
    # GIVEN an error code and message
    # WHEN the failed() factory builds the result
    result = AIPResult.failed("E001", "something broke")

    # THEN both land on the error fields, not on the text field
    assert result.status == "failed"
    assert result.error_code == "E001"
    assert result.error_message == "something broke"


def test_aip_result_input_required():
    """AIPResult.input_required() returns status='input_required' with prompt."""
    # GIVEN a question for the human and its context
    # WHEN the input_required() factory builds the result
    result = AIPResult.input_required("Approve?", {"amount": 100})

    # THEN prompt and context are both carried
    assert result.status == "input_required"
    assert result.input_prompt == "Approve?"
    assert result.input_context == {"amount": 100}


def test_aip_result_input_required_no_context():
    """AIPResult.input_required() works without context."""
    # GIVEN a question for the human and no context
    # WHEN the input_required() factory builds the result
    result = AIPResult.input_required("Continue?")

    # THEN the context stays None rather than becoming an empty dict
    assert result.status == "input_required"
    assert result.input_prompt == "Continue?"
    assert result.input_context is None


def test_aip_result_to_dict_minimal():
    """to_dict() omits None fields."""
    # GIVEN a completed result with nothing but text
    result = AIPResult.completed("ok")

    # WHEN it is serialised
    d = result.to_dict()

    # THEN only the set fields appear on the wire
    assert d == {"status": "completed", "text": "ok"}
    assert "error_code" not in d
    assert "error_message" not in d
    assert "input_prompt" not in d
    assert "input_context" not in d
    assert "data" not in d


def test_aip_result_to_dict_full():
    """to_dict() includes all set fields."""
    # GIVEN a failed result with a code and a message
    result = AIPResult.failed("E002", "timeout")

    # WHEN it is serialised
    d = result.to_dict()

    # THEN both error fields appear and the absent text field does not
    assert d == {
        "status": "failed",
        "error_code": "E002",
        "error_message": "timeout",
    }


def test_aip_result_to_dict_with_data():
    """to_dict() includes non-empty data."""
    # GIVEN a completed result carrying a non-empty data dict
    result = AIPResult.completed("ok", data={"items": 3})

    # WHEN it is serialised
    d = result.to_dict()

    # THEN the data survives serialisation
    assert d["data"] == {"items": 3}


def test_import_aipresult_from_types():
    """AIPResult remains importable from apollia.types (legacy compat)."""
    # GIVEN the legacy import path used by agents written before the split
    # WHEN AIPResult is imported through it
    from apollia.types import AIPResult as R

    # THEN it is the same class, not a re-declared copy
    assert R is AIPResult


def test_version():
    """__version__ is exposed and tracks the Apollia OS product version."""
    # GIVEN the package root
    # WHEN the version is imported
    from apollia import __version__

    # THEN it matches the product version this SDK ships with
    assert __version__ == "0.1.0-preview"


def test_new_public_api_exports():
    """New decorator-first API is exported from apollia root."""
    # GIVEN the decorator-first public API
    # WHEN its symbols are imported from the package root
    from apollia import DomainError, NeedHumanInput, agent, on_message, orchestrated, skill

    # THEN the decorators are callable and the error types are exported
    assert all(callable(x) for x in (agent, skill, on_message, orchestrated))
    assert DomainError is not None
    assert NeedHumanInput is not None

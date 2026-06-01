"""Tests for apollia.types - AIPResult dataclass and factory methods."""

from apollia.types import AIPResult


def test_aip_result_completed():
    """AIPResult.completed() returns status='completed' with text."""
    result = AIPResult.completed("done")
    assert result.status == "completed"
    assert result.text == "done"
    assert result.data == {}


def test_aip_result_completed_with_data():
    """AIPResult.completed() accepts optional data dict."""
    result = AIPResult.completed("done", data={"key": "value"})
    assert result.status == "completed"
    assert result.data == {"key": "value"}


def test_aip_result_failed():
    """AIPResult.failed() returns status='failed' with error info."""
    result = AIPResult.failed("E001", "something broke")
    assert result.status == "failed"
    assert result.error_code == "E001"
    assert result.error_message == "something broke"


def test_aip_result_input_required():
    """AIPResult.input_required() returns status='input_required' with prompt."""
    result = AIPResult.input_required("Approve?", {"amount": 100})
    assert result.status == "input_required"
    assert result.input_prompt == "Approve?"
    assert result.input_context == {"amount": 100}


def test_aip_result_input_required_no_context():
    """AIPResult.input_required() works without context."""
    result = AIPResult.input_required("Continue?")
    assert result.status == "input_required"
    assert result.input_prompt == "Continue?"
    assert result.input_context is None


def test_aip_result_to_dict_minimal():
    """to_dict() omits None fields."""
    result = AIPResult.completed("ok")
    d = result.to_dict()
    assert d == {"status": "completed", "text": "ok"}
    assert "error_code" not in d
    assert "error_message" not in d
    assert "input_prompt" not in d
    assert "input_context" not in d
    assert "data" not in d


def test_aip_result_to_dict_full():
    """to_dict() includes all set fields."""
    result = AIPResult.failed("E002", "timeout")
    d = result.to_dict()
    assert d == {
        "status": "failed",
        "error_code": "E002",
        "error_message": "timeout",
    }


def test_aip_result_to_dict_with_data():
    """to_dict() includes non-empty data."""
    result = AIPResult.completed("ok", data={"items": 3})
    d = result.to_dict()
    assert d["data"] == {"items": 3}


def test_import_aipresult_from_types():
    """AIPResult remains importable from apollia.types (legacy compat)."""
    from apollia.types import AIPResult as R

    assert R is AIPResult


def test_version():
    """__version__ is exposed and bumped post-AgentKit rebuild."""
    from apollia import __version__

    assert __version__ == "0.5.0"


def test_new_public_api_exports():
    """New decorator-first API is exported from apollia root."""
    from apollia import agent, skill, on_message, orchestrated, DomainError, NeedHumanInput

    assert all(callable(x) for x in (agent, skill, on_message, orchestrated))
    assert DomainError is not None
    assert NeedHumanInput is not None



"""Tests for apollia.types — AIPResult dataclass and factory methods."""

import pytest
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


def test_import_from_root():
    """from apollia import AIPResult works."""
    from apollia import AIPResult as R

    assert R is not None
    assert R is AIPResult


def test_version():
    """__version__ is exposed."""
    from apollia import __version__

    assert __version__ == "0.3.0"


def test_import_conversational_agent():
    """from apollia import ConversationalAgent works without runtime."""
    from apollia import ConversationalAgent

    assert ConversationalAgent is not None


def test_import_from_agents_module():
    """from apollia.agents import AIPResult, ConversationalAgent works."""
    from apollia.agents import AIPResult, ConversationalAgent

    assert AIPResult is not None
    assert ConversationalAgent is not None


def test_conversational_agent_cannot_instantiate_without_manifest():
    """ConversationalAgent enforces manifest() implementation via ABC."""
    from apollia import ConversationalAgent

    with pytest.raises(TypeError, match="abstract"):
        ConversationalAgent()  # type: ignore[abstract]


def test_agent_manifest_dict_accepts_aip_v2_fields():
    """AgentManifestDict accepts the four AIP v2 fields."""
    from apollia.stubs.manifest import AgentManifestDict

    manifest: AgentManifestDict = {
        "name": "test",
        "agent_type": "worker",
        "examples": ["example"],
        "limitations": ["none"],
        "setup_notes": "install X",
    }
    assert manifest["agent_type"] == "worker"
    assert manifest["examples"] == ["example"]
    assert manifest["limitations"] == ["none"]
    assert manifest["setup_notes"] == "install X"

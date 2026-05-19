"""Tests for the public ``apollia`` package surface."""

from __future__ import annotations

import importlib

import pytest


def test_top_level_decorators_importable() -> None:
    import apollia

    assert callable(apollia.agent)
    assert callable(apollia.skill)
    assert callable(apollia.on_message)
    assert callable(apollia.orchestrated)


def test_exceptions_re_exported() -> None:
    from apollia import (
        AgentConfigError,
        AgentError,
        DomainError,
        NeedHumanInput,
        PayloadError,
        SchemaError,
        SkillNotFound,
    )

    # All inherit from AgentError.
    assert issubclass(DomainError, AgentError)
    assert issubclass(NeedHumanInput, AgentError)
    assert issubclass(PayloadError, AgentError)
    assert issubclass(SchemaError, AgentError)
    assert issubclass(SkillNotFound, AgentError)
    assert issubclass(AgentConfigError, AgentError)


def test_version_is_string() -> None:
    import apollia

    assert isinstance(apollia.__version__, str)
    # We bumped to 0.5.0 at LOT 2.
    assert apollia.__version__.startswith("0.5")


def test_all_contains_expected_names() -> None:
    import apollia

    expected = {
        "agent",
        "skill",
        "on_message",
        "orchestrated",
        "AgentError",
        "DomainError",
        "NeedHumanInput",
        "PayloadError",
        "SchemaError",
        "SkillNotFound",
        "AgentConfigError",
        "__version__",
    }
    assert expected.issubset(set(apollia.__all__))


def test_legacy_top_level_imports_removed() -> None:
    """Old public API is no longer re-exported from ``apollia.__init__``."""
    import apollia

    # The old API was: from apollia import WorkerAgent, BaseReActAgent, ...
    assert not hasattr(apollia, "WorkerAgent") or "WorkerAgent" not in apollia.__all__
    assert not hasattr(apollia, "BaseReActAgent") or (
        "BaseReActAgent" not in apollia.__all__
    )
    # AIPResult is now internal.
    assert "AIPResult" not in apollia.__all__


def test_legacy_modules_still_importable_for_migration() -> None:
    """LOT 9 still keeps legacy modules around for progressive migration."""
    # Direct import from the legacy submodule should still work.
    mod = importlib.import_module("apollia.agents")
    assert mod is not None


# ──────────────────────────────────────────────────────────────────────
# LOT 3 — Ctx Protocol + vision helpers
# ──────────────────────────────────────────────────────────────────────


def test_ctx_message_and_vision_helpers_top_level_import() -> None:
    """The Ctx surface and vision helpers are reachable from ``apollia``."""
    from apollia import (
        Ctx,
        ImageContent,
        LlmMessage,
        Message,
        TextContent,
        image_from_bytes,
        image_from_path,
        image_from_url,
        text,
    )

    assert Ctx is not None
    assert Message is not None
    assert LlmMessage is not None
    assert TextContent is not None
    assert ImageContent is not None
    assert callable(text)
    assert callable(image_from_path)
    assert callable(image_from_bytes)
    assert callable(image_from_url)


def test_all_contains_lot3_exports() -> None:
    import apollia

    lot3_exports = {
        "Ctx",
        "Message",
        "LlmMessage",
        "MessageContent",
        "TextContent",
        "ImageContent",
        "text",
        "image_from_path",
        "image_from_bytes",
        "image_from_url",
    }
    missing = lot3_exports - set(apollia.__all__)
    assert not missing, f"Missing exports from apollia.__all__: {missing}"

"""Tests for the public ``apollia`` package surface."""

from __future__ import annotations


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
    assert apollia.__version__.startswith("0.1.0")


def test_all_is_exactly_the_published_surface() -> None:
    """``apollia.__all__`` is a contract: additions and removals both fail here.

    An ``issubset`` floor let ``react`` and ``MapItemResult`` ship unlocked
    and would have let any future addition pass in silence. The snapshot is
    exact in both directions on purpose: changing the public surface means
    changing this test in the same commit, visibly.
    """
    import apollia

    # GIVEN the published names, frozen as an exact set
    expected = {
        # Decorators
        "agent",
        "skill",
        "on_message",
        "orchestrated",
        # ReAct utility
        "react",
        # Exceptions
        "AgentError",
        "AgentConfigError",
        "DomainError",
        "NeedHumanInput",
        "PayloadError",
        "SchemaError",
        "SkillNotFound",
        # Ctx Protocol surface
        "Ctx",
        "MapItemResult",
        # Multi-modal types
        "Message",
        "MessageContent",
        "LlmMessage",
        "TextContent",
        "ImageContent",
        # Vision helpers
        "text",
        "image_from_path",
        "image_from_bytes",
        "image_from_url",
        # Version
        "__version__",
    }

    # WHEN the live surface is read
    published = set(apollia.__all__)

    # THEN they match exactly, and __all__ carries no duplicate
    assert published == expected, (
        f"added: {sorted(published - expected)}, removed: {sorted(expected - published)}"
    )
    assert len(apollia.__all__) == len(published), "duplicate name in apollia.__all__"
    for name in published:
        assert hasattr(apollia, name), f"{name} is in __all__ but not importable"


def test_legacy_top_level_imports_removed() -> None:
    """Old public API is no longer re-exported from ``apollia.__init__``."""
    import apollia

    # The old API was: from apollia import WorkerAgent, BaseReActAgent, ...
    assert not hasattr(apollia, "WorkerAgent") or "WorkerAgent" not in apollia.__all__
    assert not hasattr(apollia, "BaseReActAgent") or ("BaseReActAgent" not in apollia.__all__)
    # AIPResult is now internal.
    assert "AIPResult" not in apollia.__all__


# ──────────────────────────────────────────────────────────────────────
# Ctx Protocol + vision helpers
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


def test_public_submodules_are_exact_snapshots() -> None:
    """The sub-surfaces sdk/AGENTS.md declares contract are frozen too.

    ``apollia.types``, ``apollia.context``, ``apollia.testing`` and
    ``apollia.utils`` are the contract the runtime injects and the mocks
    mirror; each ``__all__`` is snapshotted exactly, in both directions.
    """
    import importlib
    import pkgutil

    # GIVEN the declared surface of every public submodule
    expected: dict[str, set[str]] = {
        "apollia.errors": {
            "AgentConfigError",
            "AgentError",
            "DomainError",
            "NeedHumanInput",
            "PayloadError",
            "SchemaError",
            "SkillNotFound",
        },
        "apollia.types": {
            "A2AInterface",
            "AIPResult",
            "BudgetView",
            "Ctx",
            "DatasourcesInterface",
            "EventsInterface",
            "ImageContent",
            "ImageSourceBase64",
            "ImageSourceUrl",
            "LlmMessage",
            "LlmProxy",
            "LlmResponse",
            "Logger",
            "MailInterface",
            "MailMessage",
            "MemoryInterface",
            "Message",
            "MessageContent",
            "NotifyInterface",
            "ProfileInterface",
            "SecretsInterface",
            "SttInterface",
            "TemplatesInterface",
            "TextContent",
            "ToolProxy",
            "WorkspaceContext",
            "image_from_bytes",
            "image_from_path",
            "image_from_url",
            "text",
        },
        "apollia.testing": {
            "MockA2A",
            "MockBudget",
            "MockContext",
            "MockDatasources",
            "MockEvents",
            "MockLlmProxy",
            "MockLlmResponse",
            "MockMemory",
            "MockNotify",
            "MockProfile",
            "MockSecrets",
            "MockStt",
            "MockTemplates",
            "MockToolProxy",
            "MockWorkspace",
            "assert_emitted_thought",
            "assert_emitted_token",
            "assert_llm_called",
            "assert_memory_recorded",
            "assert_result_completed",
            "assert_result_failed",
            "assert_result_input_required",
            "assert_skill_called",
            "assert_template_rendered",
            "assert_tool_called",
            "mock",
        },
        "apollia.utils": {
            "ActionParseError",
            "AssertionSpec",
            "Citation",
            "ConfidenceLevel",
            "SourceType",
            "a2a_result_data",
            "aip_result_text",
            "assert_with_confidence",
            "build_citation_payload",
            "extract_code_block",
            "extract_json",
            "extract_xml_tag",
            "format_as_json",
            "format_as_markdown",
            "format_as_text",
            "parts_to_text",
            "safe_json_loads",
            "truncate",
            "validate_action",
        },
    }

    # WHEN each submodule's live __all__ is read
    for module_name, names in expected.items():
        module = importlib.import_module(module_name)
        published = set(module.__all__)

        # THEN it matches the snapshot exactly
        assert published == names, (
            f"{module_name}: added {sorted(published - names)}, removed {sorted(names - published)}"
        )

    # AND the per-service context modules are exactly the fifteen services
    context = importlib.import_module("apollia.context")
    submodules = {info.name for info in pkgutil.iter_modules(context.__path__)}
    assert submodules == {
        "a2a",
        "budget",
        "datasources",
        "events",
        "llm",
        "logger",
        "mail",
        "memory",
        "notify",
        "profile",
        "secrets",
        "stt",
        "templates",
        "tools",
        "workspace",
    }

"""Tests for the public ``apollia`` package surface."""

from __future__ import annotations


def test_top_level_decorators_importable() -> None:
    # GIVEN the package root
    # WHEN the two decorators are read from it
    # THEN each is callable, so none is a placeholder or a module
    import apollia

    assert callable(apollia.tool)
    assert callable(apollia.workflow)


def test_the_former_contract_is_not_published_any_more() -> None:
    """An agent is data: the package offers no way to write one in Python."""
    from pathlib import Path

    import apollia
    from apollia.context.llm import LlmProxy
    from apollia.types import Ctx

    # GIVEN the package as it is installed
    root = Path(apollia.__file__).parent

    # WHEN the names and the modules of the former contract are looked up
    former_names = {"agent", "skill", "on_message", "orchestrated", "react", "SkillNotFound"}
    former_modules = [
        "agent.py",
        "skills.py",
        "messages.py",
        "orchestration.py",
        "react.py",
        "context/a2a.py",
        "_internal/manifest.py",
        "_internal/dispatch.py",
        "cli/scaffold.py",
        "cli/inspect.py",
    ]

    # THEN no name is published, no module is shipped, and the context offers
    # neither the synchronous call between agents nor the tool loop
    assert former_names.isdisjoint(apollia.__all__)
    assert [name for name in former_modules if (root / name).exists()] == []
    assert "a2a" not in Ctx.__annotations__
    assert not hasattr(LlmProxy, "run" + "_tools")


def test_exceptions_re_exported() -> None:
    # GIVEN the error types re-exported from the package root
    # WHEN each ancestry is inspected
    # THEN all of them are rooted at AgentError
    from apollia import (
        AgentConfigError,
        AgentError,
        DomainError,
        NeedHumanInput,
        PayloadError,
        SchemaError,
    )

    # All inherit from AgentError.
    assert issubclass(DomainError, AgentError)
    assert issubclass(NeedHumanInput, AgentError)
    assert issubclass(PayloadError, AgentError)
    assert issubclass(SchemaError, AgentError)
    assert issubclass(AgentConfigError, AgentError)


def test_version_is_string() -> None:
    # GIVEN the package root
    # WHEN its version is read
    # THEN it is a string on the 0.3.1 line, not a tuple or an int
    import apollia

    assert isinstance(apollia.__version__, str)
    assert apollia.__version__.startswith("0.3.1")


def test_all_is_exactly_the_published_surface() -> None:
    """``apollia.__all__`` is a contract: additions and removals both fail here.

    An ``issubset`` floor let ``MapItemResult`` ship unlocked
    and would have let any future addition pass in silence. The snapshot is
    exact in both directions on purpose: changing the public surface means
    changing this test in the same commit, visibly.
    """
    import apollia

    # GIVEN the published names, frozen as an exact set
    expected = {
        # Decorators
        "tool",
        "workflow",
        "WorkflowCtx",
        # Workflow input types
        "LongText",
        "FileInput",
        # Exceptions
        "AgentError",
        "AgentConfigError",
        "DomainError",
        "NeedHumanInput",
        "PayloadError",
        "SchemaError",
        "StructuredOutputError",
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
    # GIVEN the package root after the decorator-first migration
    # WHEN the names of the old class-based API are looked up
    # THEN none of them is published, so the legacy surface is really gone
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
    # GIVEN the package root
    # WHEN the Ctx surface and the vision helpers are imported from it
    # THEN the types resolve and the helpers are callable
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
            "QuestionDeclined",
            "SchemaError",
            "StructuredOutputError",
            "ToolApprovalDenied",
        },
        "apollia.hitl": {
            "ApprovalPayload",
            "HitlPayload",
            "InputResponse",
            "Proposition",
            "QuestionPayload",
        },
        "apollia.types": {
            "AIPResult",
            "AgentsInterface",
            "DATE_FORMAT",
            "FILE_PATH_FORMAT",
            "FileInput",
            "InputFormat",
            "LONG_TEXT_FORMAT",
            "LongText",
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
            "UserInterface",
            "WorkflowsInterface",
            "WorkspaceContext",
            "image_from_bytes",
            "image_from_path",
            "image_from_url",
            "text",
        },
        "apollia.testing": {
            "MockAgents",
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
            "MockUser",
            "MockWorkflows",
            "MockWorkspace",
            "assert_emitted_thought",
            "assert_emitted_token",
            "assert_llm_called",
            "assert_memory_recorded",
            "assert_result_completed",
            "assert_result_failed",
            "assert_result_input_required",
            "assert_template_rendered",
            "assert_tool_called",
        },
        "apollia.utils": {
            "ActionParseError",
            "AssertionSpec",
            "Citation",
            "ConfidenceLevel",
            "SourceType",
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

    # AND the per-service context modules are exactly the services of Ctx
    context = importlib.import_module("apollia.context")
    submodules = {info.name for info in pkgutil.iter_modules(context.__path__)}
    assert submodules == {
        "agents",
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
        "user",
        "workflows",
        "workspace",
    }

"""Unit tests for onboarding-agent profile-specific system prompts."""

from __future__ import annotations

import importlib.util
import pathlib
import sys

_spec = importlib.util.spec_from_file_location(
    "onboarding_agent",
    pathlib.Path(__file__).parent / "onboarding-agent.py",
)
assert _spec is not None and _spec.loader is not None
_module = importlib.util.module_from_spec(_spec)
sys.modules["onboarding_agent"] = _module
_spec.loader.exec_module(_module)  # type: ignore[union-attr]

from onboarding_agent import (  # noqa: E402
    BUILDER_SECTION_FR,
    OPERATOR_SECTION_FR,
    build_system_prompt_with_profile,
)


def test_operator_system_prompt_contains_operator_section() -> None:
    # GIVEN a context with profile="operator"
    # WHEN the system prompt is built
    prompt = build_system_prompt_with_profile("fr", "operator")
    # THEN it contains questions about recurring tasks and supervision
    assert "tâches récurrentes" in prompt or "répétitives" in prompt
    assert "supervision" in prompt
    assert "notifications" in prompt


def test_builder_system_prompt_contains_builder_section() -> None:
    # GIVEN a context with profile="builder"
    # WHEN the system prompt is built
    prompt = build_system_prompt_with_profile("fr", "builder")
    # THEN it contains questions about Python, frameworks, and tech stack
    assert "Python" in prompt
    assert "frameworks" in prompt or "LangGraph" in prompt
    assert "stack" in prompt


def test_no_profile_uses_generic_prompt() -> None:
    # GIVEN a context without a profile
    # WHEN the system prompt is built
    prompt_none = build_system_prompt_with_profile("fr", None)
    prompt_operator = build_system_prompt_with_profile("fr", "operator")
    prompt_builder = build_system_prompt_with_profile("fr", "builder")
    # THEN it contains neither the Operator nor the Builder section
    assert OPERATOR_SECTION_FR not in prompt_none
    assert BUILDER_SECTION_FR not in prompt_none
    # AND it is shorter than the profile-specific variants
    assert len(prompt_none) < len(prompt_operator)
    assert len(prompt_none) < len(prompt_builder)

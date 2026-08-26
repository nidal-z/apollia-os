"""Tests for ``@orchestrated`` decorator."""

from __future__ import annotations

import pytest
from apollia._internal.manifest import ORCHESTRATED_ATTR
from apollia.errors import AgentConfigError
from apollia.orchestration import orchestrated


def test_orchestrated_marker_set() -> None:
    # GIVEN a class decorated with a non-empty system prompt
    @orchestrated(system_prompt="You are a researcher.")
    class A:
        pass

    # WHEN we read the marker attribute the manifest builder looks for
    cfg = getattr(A, ORCHESTRATED_ATTR)

    # THEN it carries the prompt verbatim
    assert cfg == {"system_prompt": "You are a researcher."}


def test_orchestrated_returns_class_unchanged() -> None:
    # GIVEN an undecorated class
    class Original:
        pass

    # WHEN the decorator is applied to it
    decorated = orchestrated(system_prompt="x")(Original)

    # THEN the same object comes back, so the decorator is a pure marker
    assert decorated is Original


def test_orchestrated_empty_prompt_raises() -> None:
    # GIVEN an empty system prompt
    # WHEN the decorator is applied to a class
    # THEN registration fails at decoration time
    with pytest.raises(AgentConfigError, match="non-empty"):

        @orchestrated(system_prompt="")
        class A:
            pass


def test_orchestrated_whitespace_only_prompt_raises() -> None:
    # GIVEN a system prompt made of whitespace only
    # WHEN the decorator is applied to a class
    # THEN it is rejected like the empty one, not accepted as non-empty text
    with pytest.raises(AgentConfigError, match="non-empty"):

        @orchestrated(system_prompt="   \n  ")
        class A:
            pass


def test_orchestrated_non_string_prompt_raises() -> None:
    # GIVEN a system prompt that is not a string
    # WHEN the decorator factory is called
    # THEN it refuses the type instead of stringifying it
    with pytest.raises(AgentConfigError, match="non-empty"):
        orchestrated(system_prompt=42)  # type: ignore[arg-type]  # NOSONAR S5655: intentional bad type to verify the decorator rejects non-strings


def test_orchestrated_on_non_class_raises() -> None:
    # GIVEN a valid decorator and a target that is not a class
    deco = orchestrated(system_prompt="x")

    # WHEN the decorator is applied to that target
    # THEN it refuses instead of setting the marker on an arbitrary object
    with pytest.raises(AgentConfigError, match="must decorate a class"):
        deco("not a class")  # type: ignore[arg-type]

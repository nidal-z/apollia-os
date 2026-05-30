"""Tests for ``@orchestrated`` decorator."""

from __future__ import annotations

import pytest

from apollia._internal.manifest import ORCHESTRATED_ATTR
from apollia.errors import AgentConfigError
from apollia.orchestration import orchestrated


def test_orchestrated_marker_set() -> None:
    @orchestrated(system_prompt="You are a researcher.")
    class A:
        pass

    cfg = getattr(A, ORCHESTRATED_ATTR)
    assert cfg == {"system_prompt": "You are a researcher."}


def test_orchestrated_returns_class_unchanged() -> None:
    class Original:
        pass

    decorated = orchestrated(system_prompt="x")(Original)
    assert decorated is Original


def test_orchestrated_empty_prompt_raises() -> None:
    with pytest.raises(AgentConfigError, match="non-empty"):

        @orchestrated(system_prompt="")
        class A:
            pass


def test_orchestrated_whitespace_only_prompt_raises() -> None:
    with pytest.raises(AgentConfigError, match="non-empty"):

        @orchestrated(system_prompt="   \n  ")
        class A:
            pass


def test_orchestrated_non_string_prompt_raises() -> None:
    with pytest.raises(AgentConfigError, match="non-empty"):
        orchestrated(system_prompt=42)  # type: ignore[arg-type]  # NOSONAR S5655: intentional bad type to verify the decorator rejects non-strings


def test_orchestrated_on_non_class_raises() -> None:
    deco = orchestrated(system_prompt="x")
    with pytest.raises(AgentConfigError, match="must decorate a class"):
        deco("not a class")  # type: ignore[arg-type]

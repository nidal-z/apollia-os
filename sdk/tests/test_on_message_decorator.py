"""Tests for ``@on_message`` decorator."""

from __future__ import annotations

import pytest

from apollia._internal.manifest import ON_MESSAGE_ATTR
from apollia.errors import AgentConfigError
from apollia.messages import on_message


def test_on_message_marker_set() -> None:
    @on_message
    async def fn(self: object, message: str, history: list, ctx: object) -> str:
        return "ok"

    assert getattr(fn, ON_MESSAGE_ATTR) is True


def test_on_message_returns_method_unchanged() -> None:
    async def original(  # NOSONAR
        self: object, message: str, history: list, ctx: object
    ) -> str:
        return "ok"

    decorated = on_message(original)
    assert decorated is original


def test_on_message_on_sync_method_raises() -> None:
    with pytest.raises(AgentConfigError, match="async def"):

        @on_message
        def fn(self: object, message: str, history: list, ctx: object) -> str:
            return "ok"


def test_on_message_on_async_method_ok() -> None:
    @on_message
    async def fn(self: object, message: str, history: list, ctx: object) -> str:
        return "ok"

    assert getattr(fn, ON_MESSAGE_ATTR) is True


def test_on_message_on_non_callable_raises() -> None:
    with pytest.raises(AgentConfigError, match="callable"):
        on_message("not a function")  # type: ignore[arg-type]


def test_on_message_does_not_wrap() -> None:
    """Ensure ``@on_message`` is a pure marker - no closure, no wrap."""

    async def fn(self: object, message: str, history: list, ctx: object) -> str:  # NOSONAR
        return "ok"

    fn_id_before = id(fn)
    decorated = on_message(fn)
    assert id(decorated) == fn_id_before

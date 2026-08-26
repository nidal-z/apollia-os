"""Tests for ``@on_message`` decorator."""

from __future__ import annotations

import pytest
from apollia._internal.manifest import ON_MESSAGE_ATTR
from apollia.errors import AgentConfigError
from apollia.messages import on_message


def test_on_message_marker_set() -> None:
    # GIVEN an async handler decorated with @on_message
    @on_message
    async def fn(self: object, message: str, history: list, ctx: object) -> str:
        return "ok"

    # WHEN we read the attribute the manifest builder looks for
    # THEN it is set, so the handler is discoverable
    assert getattr(fn, ON_MESSAGE_ATTR) is True


def test_on_message_returns_method_unchanged() -> None:
    # GIVEN an undecorated async handler
    async def original(  # NOSONAR
        self: object, message: str, history: list, ctx: object
    ) -> str:
        return "ok"

    # WHEN the decorator is applied to it
    decorated = on_message(original)

    # THEN the same function object comes back
    assert decorated is original


def test_on_message_on_sync_method_raises() -> None:
    # GIVEN a handler declared with def, not async def
    # WHEN the decorator is applied to it
    # THEN it is refused at decoration time rather than at dispatch time
    with pytest.raises(AgentConfigError, match="async def"):

        @on_message
        def fn(self: object, message: str, history: list, ctx: object) -> str:
            return "ok"


def test_on_message_on_async_method_ok() -> None:
    # GIVEN a handler declared with async def
    @on_message
    async def fn(self: object, message: str, history: list, ctx: object) -> str:
        return "ok"

    # WHEN we read the marker attribute
    # THEN it is set, so the async form is the accepted one
    assert getattr(fn, ON_MESSAGE_ATTR) is True


def test_on_message_on_non_callable_raises() -> None:
    # GIVEN a target that is not callable
    # WHEN the decorator is applied to it
    # THEN it refuses instead of marking an arbitrary object
    with pytest.raises(AgentConfigError, match="callable"):
        on_message("not a function")  # type: ignore[arg-type]


def test_on_message_does_not_wrap() -> None:
    """Ensure ``@on_message`` is a pure marker - no closure, no wrap."""

    # GIVEN an async handler and the identity of the function object
    async def fn(self: object, message: str, history: list, ctx: object) -> str:  # NOSONAR
        return "ok"

    fn_id_before = id(fn)

    # WHEN the decorator is applied to it
    decorated = on_message(fn)

    # THEN the identity is unchanged, so no closure was interposed
    assert id(decorated) == fn_id_before

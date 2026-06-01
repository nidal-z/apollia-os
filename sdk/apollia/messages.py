"""``@on_message`` decorator - mark a method as the conversational handler.

At most one ``@on_message`` method is allowed per agent
class; the duplication check fires at ``@agent`` decoration time
(via :func:`apollia._internal.manifest.find_on_message_handler`).
"""

from __future__ import annotations

import inspect
from collections.abc import Callable
from typing import Any, TypeVar

from apollia._internal.manifest import ON_MESSAGE_ATTR
from apollia.errors import AgentConfigError

__all__ = ["on_message"]

F = TypeVar("F", bound=Callable[..., Any])


def on_message(fn: F) -> F:
    """Mark a method as the agent's conversational handler.

    The decorated method receives ``(self, message, history, ctx)`` and
    returns a string (final assistant reply) or yields tokens via
    ``ctx.events.emit_token`` when streaming. The method must be
    ``async def``.

    At most one ``@on_message`` method per class is allowed - the
    duplicate check happens at ``@agent`` decoration time.

    Raises:
        AgentConfigError: if the decorated callable is not async or not
            a function.
    """
    if not callable(fn):
        raise AgentConfigError(
            f"@on_message must decorate a callable, got {type(fn).__name__}"
        )
    if not inspect.iscoroutinefunction(fn):
        raise AgentConfigError(
            f"@on_message: method '{getattr(fn, '__name__', '?')}' "
            "must be 'async def'"
        )
    setattr(fn, ON_MESSAGE_ATTR, True)
    return fn

"""``@orchestrated`` decorator - mark an agent as ORIA-orchestrated.

``@orchestrated`` is mutually exclusive with ``@skill`` and
``@on_message`` on the same class; the consistency check is enforced
when ``@agent`` builds the manifest.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import TypeVar

from apollia._internal.manifest import ORCHESTRATED_ATTR
from apollia.errors import AgentConfigError

__all__ = ["orchestrated"]

C = TypeVar("C", bound=type)


def orchestrated(*, system_prompt: str) -> Callable[[C], C]:
    """Mark the decorated class as ORIA-orchestrated.

    The agent provides only metadata + ``system_prompt``. ORIA handles
    the execution loop. The class may define
    ``async on_plan_complete(step_results, ctx)`` to post-process step
    outputs (defaults to concatenating step texts). The runtime calls it
    with both the step results and the runtime ``ctx``.

    ``@orchestrated`` is mutually exclusive with ``@skill`` and
    ``@on_message``. Validation happens at ``@agent`` decoration time
    (the ``@agent`` decorator triggers
    :func:`apollia._internal.manifest.build_manifest`, which enforces
    the exclusivity).

    Raises:
        AgentConfigError: if ``system_prompt`` is not a non-empty string.
    """
    if not isinstance(system_prompt, str) or not system_prompt.strip():
        raise AgentConfigError("@orchestrated requires a non-empty string system_prompt")

    def decorator(cls: C) -> C:
        if not isinstance(cls, type):
            raise AgentConfigError(f"@orchestrated must decorate a class, got {type(cls).__name__}")
        setattr(cls, ORCHESTRATED_ATTR, {"system_prompt": system_prompt})
        return cls

    return decorator

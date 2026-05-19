"""``@agent`` automatic module-level instance exposure.

ADR-107: every agent module must expose a module-level ``agent`` symbol
that points to the singleton instance of the ``@agent``-decorated class.
The decorator (LOT 2) instantiates the class and calls
:func:`expose_to_module` to bind it.
"""

from __future__ import annotations

import inspect
import sys
from typing import Any

from apollia.errors import AgentConfigError

__all__ = [
    "expose_to_module",
    "get_module_agent",
]


def _resolve_module(cls: type) -> Any:
    module = inspect.getmodule(cls)
    if module is None:
        raise AgentConfigError(
            f"Cannot resolve defining module for class {cls.__name__}"
        )
    return module


def expose_to_module(cls: type, instance: Any) -> None:
    """Bind ``instance`` to the ``agent`` attribute of the module that defines ``cls``.

    Raises :class:`AgentConfigError` if:

    - the module already has an ``agent`` attribute belonging to a
      different agent class, or
    - the defining module cannot be resolved.

    Re-binding the same agent class is a no-op (idempotent).
    """
    module = _resolve_module(cls)
    existing = getattr(module, "agent", None)
    if existing is not None and existing is not instance:
        # Tolerate the case where `module.agent` is the @agent decorator
        # itself (user did `from apollia import agent` at the top of the
        # file). The decorator is the very `agent` function exported by
        # `apollia.agent`; detect it by identity.
        try:
            from apollia.agent import agent as _agent_decorator
            is_decorator_shadow = existing is _agent_decorator
        except ImportError:
            is_decorator_shadow = False
        if not is_decorator_shadow:
            existing_cls = type(existing)
            if existing_cls is not cls:
                raise AgentConfigError(
                    f"Module '{module.__name__}' already declares an 'agent' of "
                    f"class {existing_cls.__name__}; cannot register {cls.__name__}"
                )
    setattr(module, "agent", instance)


def get_module_agent(module_name: str) -> Any | None:
    """Return the module-level ``agent`` of ``module_name`` if any.

    Test helper. Returns ``None`` if the module isn't loaded or doesn't
    expose an ``agent`` attribute.
    """
    module = sys.modules.get(module_name)
    if module is None:
        return None
    return getattr(module, "agent", None)

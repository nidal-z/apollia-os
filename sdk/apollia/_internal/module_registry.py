"""``@agent`` automatic module-level instance exposure.

Every agent module must expose a module-level ``agent`` symbol
that points to the singleton instance of the ``@agent``-decorated class.
The decorator instantiates the class and calls
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
        # Only a real agent instance — i.e. an instance whose class carries
        # the generated ``__apollia_manifest__`` marker — blocks the
        # re-registration. Everything else (the ``agent`` decorator function
        # shadowed by ``from apollia import agent``, a stale binding from a
        # previous load, an unrelated callable) is considered a harmless
        # shadow and gets overwritten silently.
        existing_cls = type(existing)
        existing_is_real_agent = hasattr(existing_cls, "__apollia_manifest__")
        if existing_is_real_agent and existing_cls is not cls:
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

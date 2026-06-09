"""``@agent`` decorator - declare a class as an Apollia agent.

The decorator:

- validates the class against the agent contract,
- builds the canonical manifest (caches it on the class),
- attaches an async ``__apollia_dispatch__`` hook for the Rust bridge,
- instantiates the class and exposes the singleton via the module's
  ``agent`` attribute.
"""

from __future__ import annotations

import inspect
from collections.abc import Callable
from typing import Any, TypeVar

from apollia._internal.dispatch import dispatch_task
from apollia._internal.manifest import (
    AGENT_META_ATTR,
    MANIFEST_ATTR,
    SKILLS_REGISTRY_ATTR,
    build_manifest,
    find_on_message_handler,
    find_orchestrated_config,
)
from apollia._internal.module_registry import expose_to_module
from apollia.errors import AgentConfigError

__all__ = ["agent"]

C = TypeVar("C", bound=type)


def _check_required_str(name: str, value: Any) -> str:
    if not isinstance(value, str) or not value.strip():
        raise AgentConfigError(f"@agent: '{name}' must be a non-empty string")
    return value


def _check_string_tuple(name: str, value: Any) -> tuple[str, ...]:
    if not isinstance(value, tuple):
        raise AgentConfigError(
            f"@agent: '{name}' must be a tuple of strings, got {type(value).__name__}"
        )
    for item in value:
        if not isinstance(item, str) or not item:
            raise AgentConfigError(f"@agent: '{name}' must contain non-empty strings, got {item!r}")
    return value


def _check_init_takes_no_required_args(cls: type) -> None:
    """Ensure ``cls()`` can be called with no arguments.

    Inspects the class signature (which resolves ``__init__`` through the
    MRO). Built-ins / un-introspectable callables are skipped - the
    subsequent ``cls()`` call will surface the real error.
    """
    try:
        sig = inspect.signature(cls)
    except (TypeError, ValueError):
        return
    params = list(sig.parameters.values())
    for p in params:
        if p.kind in (
            inspect.Parameter.VAR_POSITIONAL,
            inspect.Parameter.VAR_KEYWORD,
        ):
            continue
        if p.default is inspect.Parameter.empty:
            raise AgentConfigError(
                f"@agent class {cls.__name__}.__init__ must accept no required "
                f"arguments (besides self); '{p.name}' has no default"
            )


def _check_optional_str(name: str, value: Any) -> None:
    if value is not None and (not isinstance(value, str) or not value):
        raise AgentConfigError(f"@agent: {name!r} must be a non-empty string or None")


def _check_optional_dict(name: str, value: Any) -> None:
    if value is not None and not isinstance(value, dict):
        raise AgentConfigError(f"@agent: {name!r} must be a dict or None")


def agent(  # NOSONAR S107: public decorator API surface
    *,
    name: str,
    version: str,
    description: str,
    packages: tuple[str, ...] = (),
    tags: tuple[str, ...] = (),
    datasources: tuple[str, ...] = (),
    templates: tuple[str, ...] = (),
    secrets: tuple[str, ...] = (),
    tools_required: tuple[str, ...] = (),
    user_memory_write: bool = False,
    memory_namespace: str | None = None,
    shared_memory_namespaces: tuple[str, ...] = (),
    step_budget: dict[str, Any] | None = None,
    check_commands: tuple[str, ...] = (),
    agent_type: str | None = None,
) -> Callable[[C], C]:
    """Declare a class as an Apollia agent.

    Args:
        name: Unique agent identifier (non-empty).
        version: Semantic version string (non-empty).
        description: Human-readable description (non-empty).
        packages: Package identifiers the agent belongs to.
        tags: Free-form discovery tags.
        datasources: Required datasource ids (declared in the manifest).
        templates: Required Jinja2 template ids.
        secrets: Required secret ids.
        tools_required: Required tool registry ids.
        user_memory_write: Whether the agent may write to user memory.
        memory_namespace: Optional memory namespace name.
        shared_memory_namespaces: Additional shared memory namespaces.
        step_budget: Optional step-budget override dict.
        check_commands: Shell commands the runtime runs after the agent finishes,
            to verify its output (for example tests or linters). Declared checks
            run by default; at higher autonomy levels the agent also retries to
            fix detected failures.
        agent_type: Optional taxonomy hint (e.g. ``"worker"``).

    The decorator:

    1. Validates ``name`` / ``version`` / ``description`` are non-empty.
    2. Validates string-tuple arguments contain only non-empty strings.
    3. Validates the class ``__init__`` accepts no required positional
       args (besides ``self``).
    4. Validates ``@orchestrated`` exclusivity with ``@skill`` /
       ``@on_message`` (via :func:`build_manifest`).
    5. Validates at most one ``@on_message`` method (via
       :func:`find_on_message_handler`).
    6. Validates the agent has at least one entry point - i.e. ``@skill``,
       ``@on_message`` or ``@orchestrated``.
    7. Builds the manifest via :func:`build_manifest` and caches it as
       ``cls.__apollia_manifest__``.
    8. Stores all decorator args in ``cls.__apollia_agent_meta__``.
    9. Attaches an async ``__apollia_dispatch__(task, ctx) -> dict`` hook
       that delegates to :func:`apollia._internal.dispatch.dispatch_task`.
    10. Instantiates the class with no arguments and exposes the
        singleton via :func:`expose_to_module`.

    Raises:
        AgentConfigError: on any validation failure (fail-fast at import time).
    """
    name_v = _check_required_str("name", name)
    version_v = _check_required_str("version", version)
    description_v = _check_required_str("description", description)
    packages_v = _check_string_tuple("packages", packages)
    tags_v = _check_string_tuple("tags", tags)
    datasources_v = _check_string_tuple("datasources", datasources)
    templates_v = _check_string_tuple("templates", templates)
    secrets_v = _check_string_tuple("secrets", secrets)
    tools_required_v = _check_string_tuple("tools_required", tools_required)
    shared_memory_v = _check_string_tuple("shared_memory_namespaces", shared_memory_namespaces)
    check_commands_v = _check_string_tuple("check_commands", check_commands)
    _check_optional_str("memory_namespace", memory_namespace)
    _check_optional_dict("step_budget", step_budget)
    _check_optional_str("agent_type", agent_type)

    def decorator(cls: C) -> C:
        if not isinstance(cls, type):
            raise AgentConfigError(f"@agent must decorate a class, got {type(cls).__name__}")

        _check_init_takes_no_required_args(cls)

        # Walk markers (these raise AgentConfigError on duplicates).
        on_message_handler = find_on_message_handler(cls)
        orchestrated_cfg = find_orchestrated_config(cls)

        # Build the manifest (also enforces orchestrated/skill exclusivity
        # and stamps SKILLS_REGISTRY_ATTR / ON_MESSAGE_HANDLER_ATTR on cls).
        manifest = build_manifest(
            cls,
            name=name_v,
            version=version_v,
            description=description_v,
            packages=packages_v,
            tags=tags_v,
            datasources=datasources_v,
            templates=templates_v,
            secrets=secrets_v,
            tools_required=tools_required_v,
            user_memory_write=user_memory_write,
            memory_namespace=memory_namespace,
            shared_memory_namespaces=shared_memory_v,
            step_budget=step_budget,
            check_commands=check_commands_v,
            agent_type=agent_type,
        )

        skills_registry = getattr(cls, SKILLS_REGISTRY_ATTR, {}) or {}
        has_skill = bool(skills_registry)
        has_on_message = on_message_handler is not None
        has_orchestrated = orchestrated_cfg is not None

        if not (has_skill or has_on_message or has_orchestrated):
            raise AgentConfigError(
                f"@agent {name_v!r}: class {cls.__name__} has no handler - "
                "declare at least one @skill, @on_message or @orchestrated"
            )

        # Cache manifest + meta on the class.
        setattr(cls, MANIFEST_ATTR, manifest)
        setattr(
            cls,
            AGENT_META_ATTR,
            {
                "name": name_v,
                "version": version_v,
                "description": description_v,
                "packages": packages_v,
                "tags": tags_v,
                "datasources": datasources_v,
                "templates": templates_v,
                "secrets": secrets_v,
                "tools_required": tools_required_v,
                "user_memory_write": bool(user_memory_write),
                "memory_namespace": memory_namespace,
                "shared_memory_namespaces": shared_memory_v,
                "step_budget": step_budget,
                "check_commands": check_commands_v,
                "agent_type": agent_type,
            },
        )

        # Attach the dispatch hook the Rust bridge will call.
        async def __apollia_dispatch__(self: Any, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
            return await dispatch_task(self, task, ctx)

        __apollia_dispatch__.__qualname__ = f"{cls.__qualname__}.__apollia_dispatch__"
        cls.__apollia_dispatch__ = __apollia_dispatch__  # type: ignore[attr-defined]

        # Auto-instantiate and expose to the defining module.
        try:
            instance = cls()
        except TypeError as exc:
            raise AgentConfigError(
                f"@agent {name_v!r}: cannot instantiate {cls.__name__}() with "
                f"no arguments - {exc}"
            ) from exc

        expose_to_module(cls, instance)

        return cls

    return decorator

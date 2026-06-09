"""Manifest generation from ``@agent``, ``@skill``, ``@on_message`` and
``@orchestrated`` decorators.

The decorators stamp markers on classes and methods.
This module walks those markers to produce the canonical manifest dict
consumed by the Rust loader.
"""

from __future__ import annotations

import inspect
from typing import Any

from apollia._internal.inference import (
    return_to_output_schema,
    signature_to_input_schema,
)
from apollia.errors import AgentConfigError

__all__ = [
    "SKILL_ATTR",
    "ON_MESSAGE_ATTR",
    "ORCHESTRATED_ATTR",
    "AGENT_META_ATTR",
    "MANIFEST_ATTR",
    "SKILLS_REGISTRY_ATTR",
    "ON_MESSAGE_HANDLER_ATTR",
    "SkillEntry",
    "build_manifest",
    "collect_skills",
    "find_on_message_handler",
    "find_orchestrated_config",
]


# ──────────────────────────────────────────────────────────────────────
# Marker attributes (set by decorators)
# ──────────────────────────────────────────────────────────────────────

# Method attribute → dict {id, description, requires_approval, dangerous}.
SKILL_ATTR = "__apollia_skill__"
# Method attribute → True.
ON_MESSAGE_ATTR = "__apollia_on_message__"
# Class attribute → dict {system_prompt}.
ORCHESTRATED_ATTR = "__apollia_orchestrated__"
# Class attribute → dict (set by ``@agent``).
AGENT_META_ATTR = "__apollia_agent_meta__"
# Class attribute → cached manifest dict.
MANIFEST_ATTR = "__apollia_manifest__"
# Class attribute → dict[skill_id, SkillEntry].
SKILLS_REGISTRY_ATTR = "__apollia_skills__"
# Class attribute → method name (str) of the ``@on_message`` handler.
ON_MESSAGE_HANDLER_ATTR = "__apollia_on_message_handler__"


class SkillEntry:
    """Internal skill registration data attached to an agent class."""

    __slots__ = (
        "skill_id",
        "handler_name",
        "description",
        "input_schema",
        "output_schema",
        "requires_approval",
        "dangerous",
        "examples",
    )

    def __init__(
        self,
        skill_id: str,
        handler_name: str,
        description: str,
        input_schema: dict[str, Any],
        output_schema: dict[str, Any],
        requires_approval: bool,
        dangerous: bool,
        examples: list[dict[str, Any]] | None = None,
    ) -> None:
        self.skill_id = skill_id
        self.handler_name = handler_name
        self.description = description
        self.input_schema = input_schema
        self.output_schema = output_schema
        self.requires_approval = requires_approval
        self.dangerous = dangerous
        self.examples: list[dict[str, Any]] = list(examples) if examples else []


# ──────────────────────────────────────────────────────────────────────
# Class introspection
# ──────────────────────────────────────────────────────────────────────


def _iter_methods(cls: type) -> list[tuple[str, Any]]:
    """Return ``(name, function)`` tuples for every method of ``cls`` and
    its MRO, with subclasses overriding base classes (i.e. MRO order).
    """
    seen: set[str] = set()
    result: list[tuple[str, Any]] = []
    for klass in cls.__mro__:
        if klass is object:
            continue
        for name, member in klass.__dict__.items():
            if name in seen:
                continue
            if inspect.isfunction(member) or inspect.ismethod(member):
                seen.add(name)
                result.append((name, member))
    return result


def _docstring_first_line(fn: Any) -> str:
    """Return the first line/paragraph of a function's docstring.

    Used as the fallback description when ``@skill(description=...)`` is
    not provided. Returns ``""`` if there is no docstring.

    Algorithm:
    1. ``inspect.getdoc(fn)`` (cleans indentation).
    2. Split on the first blank line - keep the leading paragraph.
    3. Within that paragraph, keep only the first non-empty line.
    """
    raw = inspect.getdoc(fn)
    if not raw:
        return ""
    # First paragraph (until blank line).
    paragraph = raw.split("\n\n", 1)[0]
    for line in paragraph.splitlines():
        stripped = line.strip()
        if stripped:
            return stripped
    return ""


def collect_skills(cls: type) -> dict[str, SkillEntry]:
    """Walk ``cls`` (and its MRO) to collect all ``@skill``-decorated methods.

    The decorator stamps :data:`SKILL_ATTR` on the method with a dict
    ``{id, description, requires_approval, dangerous, examples}``. This function
    builds input/output JSON schemas via :mod:`apollia._internal.inference`
    and returns a ``{skill_id: SkillEntry}`` mapping.

    Description resolution order:

    1. Explicit ``@skill(description=...)`` argument if non-empty.
    2. First line of the handler's docstring (see :func:`_docstring_first_line`).
    3. Empty string.

    Raises :class:`AgentConfigError` if two methods declare the same
    skill id.
    """
    registry: dict[str, SkillEntry] = {}
    for name, fn in _iter_methods(cls):
        meta = getattr(fn, SKILL_ATTR, None)
        if not meta:
            continue
        skill_id = meta.get("id") or name
        if skill_id in registry:
            raise AgentConfigError(f"Duplicate @skill id '{skill_id}' on class {cls.__name__}")
        input_schema = signature_to_input_schema(fn)
        output_schema = return_to_output_schema(fn)
        explicit_desc = meta.get("description") or ""
        description = explicit_desc if explicit_desc else _docstring_first_line(fn)
        examples_meta = meta.get("examples") or []
        registry[skill_id] = SkillEntry(
            skill_id=skill_id,
            handler_name=name,
            description=description,
            input_schema=input_schema,
            output_schema=output_schema,
            requires_approval=bool(meta.get("requires_approval", False)),
            dangerous=bool(meta.get("dangerous", False)),
            examples=list(examples_meta) if isinstance(examples_meta, list) else [],
        )
    return registry


def find_on_message_handler(cls: type) -> str | None:
    """Return the name of the ``@on_message``-decorated method, or ``None``.

    Raises :class:`AgentConfigError` if more than one method is decorated.
    """
    found: list[str] = []
    for name, fn in _iter_methods(cls):
        if getattr(fn, ON_MESSAGE_ATTR, False):
            found.append(name)
    if not found:
        return None
    if len(found) > 1:
        raise AgentConfigError(
            f"Class {cls.__name__} declares multiple @on_message handlers: {found}"
        )
    return found[0]


def find_orchestrated_config(cls: type) -> dict[str, Any] | None:
    """Return the ``@orchestrated`` config dict, or ``None``."""
    for klass in cls.__mro__:
        if klass is object:
            continue
        cfg = klass.__dict__.get(ORCHESTRATED_ATTR)
        if cfg is not None:
            if not isinstance(cfg, dict):
                raise AgentConfigError(f"{ORCHESTRATED_ATTR} on {klass.__name__} must be a dict")
            return cfg
    return None


# ──────────────────────────────────────────────────────────────────────
# Manifest assembly
# ──────────────────────────────────────────────────────────────────────


def _check_string_tuple(name: str, values: tuple[str, ...]) -> list[str]:
    out: list[str] = []
    for v in values:
        if not isinstance(v, str) or not v:
            raise AgentConfigError(
                f"{name} must be a tuple of non-empty strings, got element {v!r}"
            )
        out.append(v)
    return out


def build_manifest(
    cls: type,
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
) -> dict[str, Any]:
    """Produce the canonical manifest dict consumed by the Rust loader.

    Validates at load time (fail-fast):

    - ``name`` and ``version`` are non-empty.
    - Every entry in ``datasources``/``templates``/``secrets``/``tools_required``
      is a non-empty string.
    - Skill ids are unique.
    - ``@orchestrated`` and ``@skill``/``@on_message`` cannot coexist on
      the same class.
    """
    if not isinstance(name, str) or not name:
        raise AgentConfigError("manifest 'name' must be a non-empty string")
    if not isinstance(version, str) or not version:
        raise AgentConfigError("manifest 'version' must be a non-empty string")

    packages_l = _check_string_tuple("packages", packages)
    tags_l = _check_string_tuple("tags", tags)
    datasources_l = _check_string_tuple("datasources", datasources)
    templates_l = _check_string_tuple("templates", templates)
    secrets_l = _check_string_tuple("secrets", secrets)
    tools_l = _check_string_tuple("tools_required", tools_required)
    shared_mem_l = _check_string_tuple("shared_memory_namespaces", shared_memory_namespaces)

    skills_registry = collect_skills(cls)
    on_message_handler = find_on_message_handler(cls)
    orchestrated_cfg = find_orchestrated_config(cls)

    if orchestrated_cfg is not None and (skills_registry or on_message_handler):
        raise AgentConfigError(
            f"Class {cls.__name__} cannot mix @orchestrated with @skill or @on_message"
        )

    # Cache the registry / on_message handler name on the class so dispatch
    # can find them at runtime without re-walking the MRO.
    setattr(cls, SKILLS_REGISTRY_ATTR, skills_registry)
    if on_message_handler is not None:
        setattr(cls, ON_MESSAGE_HANDLER_ATTR, on_message_handler)

    # Determine execution mode.
    if orchestrated_cfg is not None:
        execution_mode = "orchestrated"
    elif on_message_handler is not None:
        execution_mode = "conversational"
    else:
        execution_mode = "direct"

    skills_list: list[dict[str, Any]] = []
    for entry in skills_registry.values():
        skill_dict: dict[str, Any] = {
            "id": entry.skill_id,
            "name": entry.skill_id,
            "description": entry.description,
            # Default I/O modes for A2A skills generated by @skill:
            # structured payloads (DataPart). Agents that need text-only
            # or file-only modes can override via a future @skill option.
            "input_modes": ["data"],
            "output_modes": ["data"],
            "input_schema": entry.input_schema,
            "output_schema": entry.output_schema,
            "requires_approval": entry.requires_approval,
            "dangerous": entry.dangerous,
        }
        # Only include examples when the author actually provided some, to
        # avoid noising the manifest with empty arrays for skills that
        # don't use them.
        if entry.examples:
            skill_dict["examples"] = list(entry.examples)
        skills_list.append(skill_dict)

    manifest: dict[str, Any] = {
        "name": name,
        "version": version,
        "description": description,
        "packages": packages_l,
        "tags": tags_l,
        "datasources": datasources_l,
        "templates": templates_l,
        "secrets": secrets_l,
        "tools_required": tools_l,
        "user_memory_write": bool(user_memory_write),
        "memory_namespace": memory_namespace,
        "shared_memory_namespaces": shared_mem_l,
        "step_budget": step_budget,
        "check_commands": list(check_commands),
        "agent_type": agent_type,
        "supports_a2a": len(skills_registry) > 0,
        "skills": skills_list,
        "execution_mode": execution_mode,
    }

    if orchestrated_cfg is not None:
        manifest["system_prompt"] = orchestrated_cfg.get("system_prompt", "")

    return manifest

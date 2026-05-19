"""Task dispatch + exception trap boundary.

Routes an incoming AIP task to the right handler on an agent instance,
validates the payload against the inferred schema, awaits coroutine
handlers, and traps every exception into a canonical ``AIPResult`` dict.
See ADR-098 and ADR-100.
"""

from __future__ import annotations

import inspect
import json
import logging
from typing import Any

from apollia._internal.aip_result import (
    failed,
    from_exception,
    from_handler_return,
)
from apollia._internal.inference import validate_payload
from apollia._internal.manifest import (
    ON_MESSAGE_HANDLER_ATTR,
    SKILLS_REGISTRY_ATTR,
    SkillEntry,
)
from apollia.errors import SkillNotFound

__all__ = [
    "dispatch_task",
    "dispatch_skill",
    "dispatch_message",
    "extract_task_message",
    "extract_task_skill_id",
    "extract_task_payload",
]


# ──────────────────────────────────────────────────────────────────────
# Task field extraction helpers
# ──────────────────────────────────────────────────────────────────────


def extract_task_message(task: dict[str, Any]) -> str:
    """Return the first text part of the task input, or ``""``."""
    try:
        parts = task["input"]["parts"]
    except (KeyError, TypeError):
        return ""
    if not isinstance(parts, list):
        return ""
    for part in parts:
        if isinstance(part, dict) and part.get("type") == "text":
            text = part.get("text", "")
            if isinstance(text, str):
                return text
    return ""


def extract_task_skill_id(task: dict[str, Any]) -> str | None:
    """Return ``task['skill_id']`` if present and non-empty, else ``None``."""
    sid = task.get("skill_id")
    if isinstance(sid, str) and sid:
        return sid
    return None


def extract_task_payload(task: dict[str, Any]) -> dict[str, Any]:
    """Extract the structured A2A payload from a task.

    Preference order (mirrors ``apollia.utils.a2a.extract_a2a_payload``):

    1. First ``DataPart`` ⇒ its ``data`` dict.
    2. First ``TextPart`` whose ``text`` parses as a JSON object ⇒ that dict.
    3. Else ⇒ ``{}``.
    """
    try:
        parts = task["input"]["parts"]
    except (KeyError, TypeError):
        return {}
    if not isinstance(parts, list):
        return {}
    text_fallback: dict[str, Any] | None = None
    for part in parts:
        if not isinstance(part, dict):
            continue
        ptype = part.get("type")
        if ptype == "data":
            data = part.get("data")
            if isinstance(data, dict):
                return data
        elif ptype == "text" and text_fallback is None:
            text = part.get("text", "")
            if isinstance(text, str) and text.strip():
                try:
                    decoded = json.loads(text)
                except (json.JSONDecodeError, ValueError):
                    decoded = None
                if isinstance(decoded, dict):
                    text_fallback = decoded
    if text_fallback is not None:
        return text_fallback
    return {}


# ──────────────────────────────────────────────────────────────────────
# Dispatch primitives
# ──────────────────────────────────────────────────────────────────────


def _logger_from_ctx(ctx: Any) -> logging.Logger | None:
    log = getattr(ctx, "logger", None)
    if isinstance(log, logging.Logger):
        return log
    return None


async def _maybe_await(value: Any) -> Any:
    if inspect.iscoroutine(value):
        return await value
    return value


async def dispatch_skill(
    agent_instance: Any,
    skill_id: str,
    payload: dict[str, Any],
    ctx: Any,
) -> dict[str, Any]:
    """Look up a skill, validate the payload, call its handler.

    Steps:

    1. Resolve the skill via :data:`SKILLS_REGISTRY_ATTR`. Missing ⇒
       :class:`SkillNotFound` ⇒ ``UNKNOWN_SKILL_ID``.
    2. Validate the payload against ``skill.input_schema``.
       Mismatch ⇒ :class:`PayloadError` ⇒ ``PAYLOAD_ERROR``.
    3. Call the handler with ``**kwargs`` and ``ctx=ctx`` (await if coro).
    4. Wrap the return value via :func:`from_handler_return`.
    5. Trap any exception via :func:`from_exception`.
    """
    logger = _logger_from_ctx(ctx)
    try:
        registry = getattr(agent_instance, SKILLS_REGISTRY_ATTR, None)
        # Fallback to the class for compatibility (registry lives on the class).
        if registry is None:
            registry = getattr(type(agent_instance), SKILLS_REGISTRY_ATTR, {})
        if not isinstance(registry, dict) or skill_id not in registry:
            known = list(registry.keys()) if isinstance(registry, dict) else []
            raise SkillNotFound(skill_id, known)

        entry: SkillEntry = registry[skill_id]
        kwargs = validate_payload(payload, entry.input_schema)

        handler = getattr(agent_instance, entry.handler_name)
        sig = inspect.signature(handler)
        if "ctx" in sig.parameters:
            kwargs["ctx"] = ctx

        result = handler(**kwargs)
        result = await _maybe_await(result)
        return from_handler_return(result)
    except BaseException as exc:  # noqa: BLE001
        if isinstance(exc, (KeyboardInterrupt, SystemExit)):
            raise
        return from_exception(exc, logger=logger)


async def dispatch_message(
    agent_instance: Any,
    message: str,
    history: list[dict[str, Any]] | None,
    ctx: Any,
) -> dict[str, Any]:
    """Call the agent's ``@on_message`` handler.

    Handler signature: ``(self, message: str, history: list[Message], ctx: Ctx)``.
    Return value is processed by :func:`from_handler_return`; exceptions
    are trapped via :func:`from_exception`.
    """
    logger = _logger_from_ctx(ctx)
    try:
        handler_name = getattr(
            type(agent_instance), ON_MESSAGE_HANDLER_ATTR, None
        ) or getattr(agent_instance, ON_MESSAGE_HANDLER_ATTR, None)
        if handler_name is None:
            return failed("NO_HANDLER", "agent has no @on_message handler")
        handler = getattr(agent_instance, handler_name)
        sig = inspect.signature(handler)
        kwargs: dict[str, Any] = {}
        if "message" in sig.parameters:
            kwargs["message"] = message
        if "history" in sig.parameters:
            kwargs["history"] = history if history is not None else []
        if "ctx" in sig.parameters:
            kwargs["ctx"] = ctx
        # Fall back to positional if the handler uses different param names.
        if not kwargs:
            result = handler(message, history if history is not None else [], ctx)
        else:
            result = handler(**kwargs)
        result = await _maybe_await(result)
        return from_handler_return(result)
    except BaseException as exc:  # noqa: BLE001
        if isinstance(exc, (KeyboardInterrupt, SystemExit)):
            raise
        return from_exception(exc, logger=logger)


async def dispatch_task(
    agent_instance: Any,
    task: dict[str, Any],
    ctx: Any,
) -> dict[str, Any]:
    """Top-level dispatcher: route based on the shape of ``task``.

    1. If ``task.skill_id`` is set ⇒ :func:`dispatch_skill`.
    2. Else if the agent has an ``@on_message`` handler ⇒
       :func:`dispatch_message` with the first text part.
    3. Else ⇒ ``AIPResult.failed("NO_HANDLER", ...)``.

    Exceptions are always trapped — the returned dict is guaranteed to
    be a valid ``AIPResult``.
    """
    logger = _logger_from_ctx(ctx)
    try:
        skill_id = extract_task_skill_id(task)
        if skill_id is not None:
            payload = extract_task_payload(task)
            return await dispatch_skill(agent_instance, skill_id, payload, ctx)

        has_on_message = getattr(
            type(agent_instance), ON_MESSAGE_HANDLER_ATTR, None
        ) or getattr(agent_instance, ON_MESSAGE_HANDLER_ATTR, None)
        if has_on_message:
            message = extract_task_message(task)
            history = task.get("history") if isinstance(task.get("history"), list) else []
            return await dispatch_message(agent_instance, message, history, ctx)

        return failed(
            "NO_HANDLER",
            "agent has neither @skill nor @on_message handler for this task",
        )
    except BaseException as exc:  # noqa: BLE001
        if isinstance(exc, (KeyboardInterrupt, SystemExit)):
            raise
        return from_exception(exc, logger=logger)

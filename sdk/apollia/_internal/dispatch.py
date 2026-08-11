"""Task dispatch + exception trap boundary.

Routes an incoming AIP task to the right handler on an agent instance,
validates the payload against the inferred schema, awaits coroutine
handlers, and traps every exception into a canonical ``AIPResult`` dict.
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
    "dispatch_message",
    "dispatch_skill",
    "dispatch_task",
    "extract_task_message",
    "extract_task_payload",
    "extract_task_skill_id",
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


def _normalize_history(raw: object) -> list[dict[str, str]]:
    """Convert AIP task history into the SDK ``Message`` contract.

    The runtime serializes conversation history as AIP messages shaped
    ``{"role": "user"|"agent", "parts": [{"type": "text", "text": ...}]}``, but
    an ``@on_message`` handler receives ``history: list[Message]`` where each
    ``Message`` is ``{"role": "user"|"assistant", "content": str}``. Without
    this adaptation the handler reads an empty ``content`` and loses the whole
    conversation, so a conversational agent restarts every turn (and drops the
    language it had detected). Flatten the text parts into ``content`` and map
    the ``"agent"`` role to ``"assistant"``. An already-normalized message
    (``content`` present) is passed through unchanged.
    """
    if not isinstance(raw, list):
        return []
    normalized: list[dict[str, str]] = []
    for message in raw:
        if not isinstance(message, dict):
            continue
        raw_role = message.get("role")
        role = "assistant" if raw_role in ("agent", "assistant") else "user"
        content = message.get("content")
        if not isinstance(content, str) or not content:
            parts = message.get("parts")
            texts: list[str] = []
            if isinstance(parts, list):
                for part in parts:
                    if isinstance(part, dict) and part.get("type") == "text":
                        text = part.get("text")
                        if isinstance(text, str):
                            texts.append(text)
            content = "".join(texts)
        normalized.append({"role": role, "content": content})
    return normalized


def extract_task_skill_id(task: dict[str, Any]) -> str | None:
    """Return ``task['skill_id']`` if present and non-empty, else ``None``."""
    sid = task.get("skill_id")
    if isinstance(sid, str) and sid:
        return sid
    return None


def _data_part_payload(part: dict[str, Any]) -> dict[str, Any] | None:
    """Return the ``data`` dict of a ``DataPart``, or ``None``."""
    if part.get("type") != "data":
        return None
    data = part.get("data")
    return data if isinstance(data, dict) else None


def _text_part_payload(part: dict[str, Any]) -> dict[str, Any] | None:
    """Return the parsed JSON object from a ``TextPart``, or ``None``."""
    if part.get("type") != "text":
        return None
    text = part.get("text", "")
    if not isinstance(text, str) or not text.strip():
        return None
    try:
        decoded = json.loads(text)
    except ValueError:
        return None
    return decoded if isinstance(decoded, dict) else None


def extract_task_payload(task: dict[str, Any]) -> dict[str, Any]:
    """Extract the structured A2A payload from a task.

    Preference order:

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
        data_payload = _data_part_payload(part)
        if data_payload is not None:
            return data_payload
        if text_fallback is None:
            text_payload = _text_part_payload(part)
            if text_payload is not None:
                text_fallback = text_payload
    return text_fallback if text_fallback is not None else {}


# ──────────────────────────────────────────────────────────────────────
# Dispatch primitives
# ──────────────────────────────────────────────────────────────────────


def _logger_from_ctx(ctx: object) -> logging.Logger | None:
    log = getattr(ctx, "logger", None)
    if isinstance(log, logging.Logger):
        return log
    return None


async def _maybe_await(value: object) -> object:
    if inspect.iscoroutine(value):
        return await value
    return value


async def dispatch_skill(
    agent_instance: object,
    skill_id: str,
    payload: dict[str, Any],
    ctx: object,
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
    except BaseException as exc:
        if isinstance(exc, (KeyboardInterrupt, SystemExit)):
            raise
        return from_exception(exc, logger=logger)


async def dispatch_message(
    agent_instance: object,
    message: str,
    history: list[dict[str, Any]] | None,
    ctx: object,
) -> dict[str, Any]:
    """Call the agent's ``@on_message`` handler.

    Handler signature: ``(self, message: str, history: list[Message], ctx: Ctx)``.
    Return value is processed by :func:`from_handler_return`; exceptions
    are trapped via :func:`from_exception`.
    """
    logger = _logger_from_ctx(ctx)
    try:
        handler_name = getattr(type(agent_instance), ON_MESSAGE_HANDLER_ATTR, None) or getattr(
            agent_instance, ON_MESSAGE_HANDLER_ATTR, None
        )
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
    except BaseException as exc:
        if isinstance(exc, (KeyboardInterrupt, SystemExit)):
            raise
        return from_exception(exc, logger=logger)


async def dispatch_task(
    agent_instance: object,
    task: dict[str, Any],
    ctx: object,
) -> dict[str, Any]:
    """Top-level dispatcher: route based on the shape of ``task``.

    1. If ``task.skill_id`` is set ⇒ :func:`dispatch_skill`.
    2. Else if the agent has an ``@on_message`` handler ⇒
       :func:`dispatch_message` with the first text part.
    3. Else ⇒ ``AIPResult.failed("NO_HANDLER", ...)``.

    Exceptions are always trapped - the returned dict is guaranteed to
    be a valid ``AIPResult``.
    """
    logger = _logger_from_ctx(ctx)
    try:
        skill_id = extract_task_skill_id(task)
        if skill_id is not None:
            payload = extract_task_payload(task)
            return await dispatch_skill(agent_instance, skill_id, payload, ctx)

        has_on_message = getattr(type(agent_instance), ON_MESSAGE_HANDLER_ATTR, None) or getattr(
            agent_instance, ON_MESSAGE_HANDLER_ATTR, None
        )
        if has_on_message:
            message = extract_task_message(task)
            history = _normalize_history(task.get("history"))
            return await dispatch_message(agent_instance, message, history, ctx)

        return failed(
            "NO_HANDLER",
            "agent has neither @skill nor @on_message handler for this task",
        )
    except BaseException as exc:
        if isinstance(exc, (KeyboardInterrupt, SystemExit)):
            raise
        return from_exception(exc, logger=logger)

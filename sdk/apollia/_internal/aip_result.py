"""Internal ``AIPResult`` builders strictly aligned on ``apollia_core::AIPResult``.

Agents never construct these dicts directly: handlers return plain Python
values (strings, dicts, dataclasses…) or raise typed exceptions, and the
SDK dispatch boundary translates that into the canonical shape consumed by
the Rust runtime.

Reference shape (mirrors ``crates/apollia-core/src/result.rs``):

.. code-block:: python

    {
        "task_id": "",
        "status": "completed" | "failed" | "input_required",
        "output": [AIPPart, ...],
        "error": {"code", "message", "details"} | None,
        "artifacts": [],
        "input_required_data": {"prompt", "context"} | None,
    }
"""

from __future__ import annotations

import base64
import dataclasses
import json
import logging
from typing import Any

from apollia.errors import (
    AgentConfigError,
    DomainError,
    NeedHumanInput,
    PayloadError,
    SchemaError,
    SkillNotFound,
)

__all__ = [
    "completed",
    "failed",
    "input_required",
    "from_handler_return",
    "from_exception",
]


def _text_part(text: str) -> dict[str, Any]:
    return {"type": "text", "text": text}


def _data_part(data: Any) -> dict[str, Any]:
    return {"type": "data", "data": data}


def completed(
    text: str = "",
    *,
    data: Any = None,
    artifacts: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    """Build a ``completed`` AIPResult dict.

    - If only ``text`` is provided the output is a single ``TextPart``.
    - If only ``data`` is provided (not ``None``) the output is a single
      ``DataPart``.
    - If both are provided the output contains the ``TextPart`` followed
      by the ``DataPart``.
    - If neither is provided the output is empty.
    """
    output: list[dict[str, Any]] = []
    if data is not None:
        if text:
            output.append(_text_part(text))
        output.append(_data_part(data))
    else:
        if text:
            output.append(_text_part(text))
    return {
        "task_id": "",
        "status": "completed",
        "output": output,
        "error": None,
        "artifacts": list(artifacts) if artifacts else [],
        "input_required_data": None,
    }


def failed(
    code: str,
    message: str,
    details: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Build a ``failed`` AIPResult dict with structured error."""
    return {
        "task_id": "",
        "status": "failed",
        "output": [],
        "error": {
            "code": code,
            "message": message,
            "details": details,
        },
        "artifacts": [],
        "input_required_data": None,
    }


def input_required(
    prompt: str,
    context: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Build an ``input_required`` AIPResult dict (HITL suspend)."""
    return {
        "task_id": "",
        "status": "input_required",
        "output": [],
        "error": None,
        "artifacts": [],
        "input_required_data": {
            "prompt": prompt,
            "context": context if context is not None else {},
        },
    }


def from_handler_return(value: Any) -> dict[str, Any]:
    """Coerce a handler return value into a ``completed`` AIPResult dict.

    Mapping:
    - ``None`` → empty completed result
    - ``str`` → completed(text=value)
    - ``bytes`` → completed(data={"__bytes__": <base64 ascii>})
    - ``dataclass`` → completed(data=dataclasses.asdict(value))
    - ``dict`` or ``list`` → completed(data=value)
    - ``tuple`` (NamedTuple or plain) → completed(data=...)
    - anything else → completed(text=str(value))

    Always returns a valid AIPResult dict in the canonical Rust shape.
    """
    if value is None:
        return completed()

    if isinstance(value, str):
        return completed(text=value)

    if isinstance(value, (bytes, bytearray)):
        encoded = base64.b64encode(bytes(value)).decode("ascii")
        return completed(data={"__bytes__": encoded})

    # dataclass instance (but not the class itself).
    if dataclasses.is_dataclass(value) and not isinstance(value, type):
        return completed(data=dataclasses.asdict(value))

    if isinstance(value, dict):
        return completed(data=value)

    if isinstance(value, list):
        return completed(data=value)

    # NamedTuple (has _asdict) or plain tuple.
    if isinstance(value, tuple):
        as_dict = getattr(value, "_asdict", None)
        if callable(as_dict):
            return completed(data=dict(as_dict()))
        return completed(data=list(value))

    if isinstance(value, (int, float, bool)):
        # Booleans, integers, floats: serialize as text for readability.
        # Callers wanting structured numeric data should return a dict.
        return completed(text=str(value))

    return completed(text=str(value))


def from_exception(
    exc: BaseException,
    *,
    logger: logging.Logger | None = None,
) -> dict[str, Any]:
    """Map an exception to an AIPResult ``failed`` / ``input_required`` dict.

    Mapping (precedence as listed):

    - :class:`DomainError` → ``failed(code, message, details)``
    - :class:`NeedHumanInput` → ``input_required(prompt, context)``
    - :class:`PayloadError` → ``failed("PAYLOAD_ERROR", message, {field, **details})``
    - :class:`SchemaError` → ``failed("SCHEMA_ERROR", str(exc))``
    - :class:`SkillNotFound` → ``failed("UNKNOWN_SKILL_ID", str(exc), {"requested", "known"})``
    - :class:`AgentConfigError` → ``failed("AGENT_CONFIG_ERROR", str(exc))``
    - any other ``Exception`` → ``failed("EXECUTION_FAILED", str(exc))`` and
      logged via ``logger.exception`` when a logger is provided.
    """
    if isinstance(exc, DomainError):
        return failed(exc.code, exc.message, exc.details)

    if isinstance(exc, NeedHumanInput):
        return input_required(exc.prompt, exc.context)

    if isinstance(exc, PayloadError):
        details: dict[str, Any] = {}
        if exc.field is not None:
            details["field"] = exc.field
        if exc.details:
            for k, v in exc.details.items():
                # Don't overwrite explicit "field" from the exception.
                if k not in details:
                    details[k] = v
        return failed("PAYLOAD_ERROR", exc.message, details if details else None)

    if isinstance(exc, SchemaError):
        return failed("SCHEMA_ERROR", str(exc))

    if isinstance(exc, SkillNotFound):
        return failed(
            "UNKNOWN_SKILL_ID",
            str(exc),
            {"requested": exc.skill_id, "known": list(exc.known)},
        )

    if isinstance(exc, AgentConfigError):
        return failed("AGENT_CONFIG_ERROR", str(exc))

    if logger is not None:
        logger.exception("agent handler raised an unhandled exception")
    return failed("EXECUTION_FAILED", str(exc))


# Avoid an unused-import warning while keeping the module surface stable.
_ = json

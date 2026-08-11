"""Bridge stdlib ``logging`` → ``ctx.log()`` Rust tracing.

The Rust bridge (`apollia-aip/src/bridge.rs::call_run`) invokes
:func:`configure_agent_logger` once per task, *before* the agent's
``__apollia_dispatch__`` / ``run`` coroutine starts. From then on,
``ctx.logger.info("…")`` from agent code pipes each record into
``ctx.log(level, message)`` - which in turn:

1. Writes a structured ``tracing::*`` record on stderr (ops compatibility).
2. Emits a ``RuntimeEvent::AgentLog`` on the runtime EventBus, picked up
   by the ``EventPersistor`` and exposed via
   ``GET /api/v1/tasks/{task_id}/trace``.

The handler is **idempotent**: configuring the same logger twice (e.g. when
two tasks run back-to-back on the same agent) does not duplicate records.
Re-running ``configure_agent_logger`` swaps the previous handler's ``ctx``
reference so log records always reach the *current* task's RuntimeContext.

This module is **internal**: agents never import it directly. The handler
configuration is opaque from the public Ctx Protocol's perspective -
agents only see ``ctx.logger`` returning a standard :class:`logging.Logger`.
"""

from __future__ import annotations

import logging
from typing import ClassVar, Protocol


class _CtxWithLog(Protocol):
    """Minimal duck type accepted by :class:`CtxLogHandler`.

    Only the ``log(level, message)`` method is required. The full
    :class:`apollia.context.Ctx` Protocol is a superset.
    """

    def log(self, level: str, message: str) -> None: ...


class KwargTolerantLogger(logging.Logger):
    """A :class:`logging.Logger` that tolerates structured keyword arguments.

    Agents naturally write structured logs such as
    ``ctx.logger.info("summarized", count=3, error=exc)``. Stdlib logging only
    accepts a fixed kwargs set and raises ``TypeError`` on anything else, which
    crashes the agent mid-task (observed as
    ``Logger._log() got an unexpected keyword argument 'error'``). This subclass
    folds any non-standard kwargs into the message so the structured-logging
    habit is harmless instead of fatal.
    """

    #: Keyword arguments the stdlib logging methods genuinely accept.
    _STD_LOG_KWARGS = frozenset({"exc_info", "stack_info", "stacklevel", "extra"})

    def _log(self, level: int, msg: object, args: object, **kwargs: object) -> None:  # type: ignore[override]
        extra_fields = {
            key: kwargs.pop(key) for key in list(kwargs) if key not in self._STD_LOG_KWARGS
        }
        if extra_fields:
            suffix = " ".join(f"{key}={value!r}" for key, value in extra_fields.items())
            msg = f"{msg} {suffix}" if msg else suffix
        super()._log(level, msg, args, **kwargs)  # type: ignore[arg-type]


class CtxLogHandler(logging.Handler):
    """Pipe each :class:`logging.LogRecord` to ``ctx.log(level, msg)``.

    The level name is normalised to the four strings accepted by the Rust
    side (``debug``/``info``/``warn``/``error``). ``CRITICAL`` maps to
    ``error`` (no separate Rust level), and any unknown numeric level
    falls back to ``info``.

    Exceptions raised by ``ctx.log`` are absorbed via
    :meth:`logging.Handler.handleError` - telemetry must never break the
    agent loop (same invariant as ``_emit_safe`` in ``apollia.agents.react``).
    """

    #: Mapping ``logging`` numeric level → Rust ``ctx.log`` level string.
    LEVEL_MAP: ClassVar[dict[int, str]] = {
        logging.DEBUG: "debug",
        logging.INFO: "info",
        logging.WARNING: "warn",
        logging.ERROR: "error",
        logging.CRITICAL: "error",
    }

    def __init__(self, ctx: _CtxWithLog) -> None:
        super().__init__()
        self._ctx = ctx

    def set_ctx(self, ctx: _CtxWithLog) -> None:
        """Swap the bound RuntimeContext.

        Used by :func:`configure_agent_logger` on re-entry so that successive
        tasks executed on the same Python agent instance always reach the
        most recent ``ctx``. Idempotent and thread-agnostic (we expect
        Python agents to run on a single asyncio loop per task).
        """
        self._ctx = ctx

    def emit(self, record: logging.LogRecord) -> None:  # pragma: no cover - thin wrapper
        try:
            level = self.LEVEL_MAP.get(record.levelno, "info")
            msg = self.format(record)
            self._ctx.log(level, msg)
        except Exception:  # - telemetry must not raise
            self.handleError(record)


def configure_agent_logger(ctx: _CtxWithLog, agent_name: str) -> logging.Logger:
    """Configure ``logging.getLogger("apollia.agent.<agent_name>")``.

    Returns the configured logger. Idempotent: calling twice for the same
    ``agent_name`` does not add a second :class:`CtxLogHandler`; instead the
    existing handler is rebound to the new ``ctx``.

    Records are **not** propagated to the root logger to avoid duplicate
    output when the host process (CLI, desktop) has its own root handler.

    Parameters
    ----------
    ctx:
        The :class:`apollia.context.Ctx` instance for the current task.
        Must expose a ``log(level: str, message: str)`` method (the Rust
        ``RuntimeContext`` does).
    agent_name:
        Logical agent name from the manifest, used as the logger suffix.
        Empty / falsy values fall back to ``"unknown"``.

    Returns:
    -------
    logging.Logger
        The shared logger instance - agents store this in ``ctx.logger``.
    """
    name_suffix = agent_name or "unknown"
    logger = logging.getLogger(f"apollia.agent.{name_suffix}")
    # Re-home the instance onto the kwarg-tolerant subclass so a structured-log
    # habit in agent code (`logger.info("msg", error=e)`) never crashes the task.
    # Safe: the subclass adds no instance state, only overrides `_log`.
    if not isinstance(logger, KwargTolerantLogger):
        logger.__class__ = KwargTolerantLogger
    logger.setLevel(logging.DEBUG)
    logger.propagate = False  # don't double-print via root logger

    existing: CtxLogHandler | None = None
    for handler in logger.handlers:
        if isinstance(handler, CtxLogHandler):
            existing = handler
            break

    if existing is not None:
        # Idempotent reconfiguration - point the handler to the new ctx
        # so a second task on the same Python interpreter doesn't keep
        # logging through a stale RuntimeContext.
        existing.set_ctx(ctx)
        return logger

    handler = CtxLogHandler(ctx)
    handler.setFormatter(logging.Formatter("%(message)s"))
    logger.addHandler(handler)
    return logger


__all__ = ["CtxLogHandler", "configure_agent_logger"]

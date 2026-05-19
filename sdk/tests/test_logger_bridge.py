"""Tests for ``apollia._internal.logger_bridge``.

Covers:

* records pipe through ``CtxLogHandler`` into the mock ``ctx.log()``;
* level mapping (DEBUG/INFO/WARN/ERROR/CRITICAL → debug/info/warn/error);
* idempotence: configuring twice doesn't duplicate handlers;
* re-configuring rebinds the handler to the new ``ctx`` (multi-task safety);
* exception inside ``ctx.log`` doesn't propagate to the agent;
* empty / falsy agent_name falls back to ``"unknown"``.
"""

from __future__ import annotations

import logging
from typing import Any

import pytest

from apollia._internal.logger_bridge import (
    CtxLogHandler,
    configure_agent_logger,
)


class MockCtx:
    """Minimal duck type fulfilling the ``_CtxWithLog`` Protocol."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, str]] = []

    def log(self, level: str, message: str) -> None:
        self.calls.append((level, message))


@pytest.fixture(autouse=True)
def _isolate_loggers() -> Any:
    """Make sure each test starts with a clean logger tree.

    ``logging.getLogger`` is process-wide cached so without cleanup a
    previous test's handler would leak into the next one and break the
    idempotence assertions.
    """
    # GIVEN: snapshot before
    yield
    # WHEN: teardown — remove every apollia.agent.* handler we installed.
    manager = logging.Logger.manager
    for name in list(manager.loggerDict.keys()):
        if name.startswith("apollia.agent."):
            logger = logging.getLogger(name)
            for handler in list(logger.handlers):
                logger.removeHandler(handler)


def test_info_record_pipes_to_ctx_log() -> None:
    # GIVEN a configured logger bound to a mock ctx
    ctx = MockCtx()
    logger = configure_agent_logger(ctx, "demo")

    # WHEN an info record is emitted
    logger.info("hello world")

    # THEN ctx.log received the matching (level, message) pair
    assert ctx.calls == [("info", "hello world")]


def test_level_mapping_covers_all_stdlib_levels() -> None:
    # GIVEN a configured logger bound to a mock ctx
    ctx = MockCtx()
    logger = configure_agent_logger(ctx, "levels")

    # WHEN records at every stdlib level are emitted
    logger.debug("d")
    logger.info("i")
    logger.warning("w")
    logger.error("e")
    logger.critical("c")

    # THEN the rust-side level strings match the expected contract.
    assert ctx.calls == [
        ("debug", "d"),
        ("info", "i"),
        ("warn", "w"),
        ("error", "e"),
        ("error", "c"),
    ]


def test_configure_is_idempotent_no_duplicate_handlers() -> None:
    # GIVEN configure_agent_logger called twice for the same agent
    ctx = MockCtx()
    logger1 = configure_agent_logger(ctx, "twice")
    logger2 = configure_agent_logger(ctx, "twice")

    # THEN it returns the same logger with a single CtxLogHandler
    assert logger1 is logger2
    handlers = [h for h in logger1.handlers if isinstance(h, CtxLogHandler)]
    assert len(handlers) == 1


def test_configure_rebinds_handler_to_new_ctx() -> None:
    # GIVEN a first configuration with ctx1
    ctx1 = MockCtx()
    logger = configure_agent_logger(ctx1, "rebind")
    logger.info("first")
    assert ctx1.calls == [("info", "first")]

    # WHEN we reconfigure with a fresh ctx2 (simulates a second task on
    # the same Python agent instance) and emit again
    ctx2 = MockCtx()
    configure_agent_logger(ctx2, "rebind")
    logger.info("second")

    # THEN ctx1 receives no further calls and ctx2 picks up the new record.
    assert ctx1.calls == [("info", "first")]
    assert ctx2.calls == [("info", "second")]


def test_handler_swallows_ctx_log_exceptions() -> None:
    # GIVEN a ctx whose log() always raises
    class BrokenCtx:
        def log(self, level: str, message: str) -> None:
            raise RuntimeError("bus down")

    handler = CtxLogHandler(BrokenCtx())
    record = logging.LogRecord(
        name="apollia.agent.broken",
        level=logging.INFO,
        pathname=__file__,
        lineno=1,
        msg="payload",
        args=None,
        exc_info=None,
    )

    # WHEN emit() is called
    # THEN no exception escapes (handleError absorbs it).
    handler.emit(record)


def test_empty_agent_name_falls_back_to_unknown() -> None:
    # GIVEN an empty agent_name
    ctx = MockCtx()
    logger = configure_agent_logger(ctx, "")

    # THEN the logger is named "apollia.agent.unknown"
    assert logger.name == "apollia.agent.unknown"


def test_logger_does_not_propagate_to_root() -> None:
    # GIVEN configure with a default ctx
    ctx = MockCtx()
    logger = configure_agent_logger(ctx, "noprop")

    # THEN propagation is disabled to avoid double-print via root logger.
    assert logger.propagate is False


def test_unknown_numeric_level_falls_back_to_info() -> None:
    # GIVEN a handler bound to a mock ctx
    ctx = MockCtx()
    handler = CtxLogHandler(ctx)

    # WHEN a record with a non-standard level number is emitted
    record = logging.LogRecord(
        name="apollia.agent.demo",
        level=42,  # not in LEVEL_MAP
        pathname=__file__,
        lineno=1,
        msg="weird",
        args=None,
        exc_info=None,
    )
    handler.emit(record)

    # THEN it falls back to "info" per the LEVEL_MAP default.
    assert ctx.calls == [("info", "weird")]

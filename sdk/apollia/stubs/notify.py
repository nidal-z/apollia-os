"""Type stubs for NotifyInterface — Python proxy to notification engine.

This class mirrors the ``#[pyclass]`` / ``#[pymethods]`` definition in
``crates/apollia-aip/src/notify.rs``.  It exists purely to provide IDE
autocompletion and ``mypy`` validation — the real implementation lives in
Rust and is injected at runtime via PyO3.
"""

from __future__ import annotations

from typing import Awaitable


class NotifyInterface:
    """Agent notification proxy exposed via ``ctx.notify``.

    Publishes notifications via the NotificationEngine.
    When no channel is configured (opt-in), all calls are silent no-ops.
    """

    def publish(
        self,
        message: str,
        *,
        severity: str = "info",
        title: str | None = None,
        channel: str | None = None,
    ) -> Awaitable[None]:
        """Emit a notification via the NotificationEngine.

        ``severity`` must be one of: ``"debug"``, ``"info"``, ``"warn"``,
        ``"warning"``, ``"error"``, ``"critical"``. Raises ``ValueError`` for
        invalid values.
        ``title`` is optional human-readable title (also used as agent context).
        ``channel`` is reserved for future per-channel routing.
        When no notification channel is configured, this is a silent no-op.
        """
        ...

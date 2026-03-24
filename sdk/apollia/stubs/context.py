"""Type stubs for RuntimeContext — execution context injected into agents.

This class mirrors the ``#[pyclass]`` / ``#[pymethods]`` definition in
``crates/apollia-aip/src/context.rs``.  It exists purely to provide IDE
autocompletion and ``mypy`` validation — the real implementation lives in
Rust and is injected at runtime via PyO3.
"""

from __future__ import annotations

from typing import Awaitable

from apollia.stubs.llm import LlmProxy
from apollia.stubs.memory import MemoryInterface
from apollia.stubs.tools import ToolProxy


class RuntimeContext:
    """Execution context injected by the Apollia runtime into ``run(task, ctx)``.

    Exposes optional capabilities: tools, LLM, memory, and inter-agent
    messaging.  Each property returns ``None`` when the capability is
    not configured for this agent.
    """

    @property
    def tools(self) -> ToolProxy | None:
        """Proxy to Rust-native tools — ``None`` if no tools allocated."""
        ...

    @property
    def llm(self) -> LlmProxy | None:
        """Proxy to the LLM router — ``None`` if no backend available."""
        ...

    @property
    def memory(self) -> MemoryInterface | None:
        """Agent memory interface — ``None`` if no namespace configured."""
        ...

    def send(
        self,
        agent_name: str,
        message: dict[str, object],
    ) -> Awaitable[None]:
        """Send a message to another agent via the inter-agent mailbox.

        Raises ``RuntimeError`` if ``supports_a2a`` is not enabled in the
        agent manifest or if the mailbox is unavailable.
        """
        ...

    def receive(
        self,
        timeout_seconds: float | None = None,
    ) -> Awaitable[dict[str, object] | None]:
        """Receive the next pending message from this agent's mailbox.

        Returns ``None`` if no message arrives within ``timeout_seconds``
        (defaults to 5.0).  Raises ``RuntimeError`` if A2A is not enabled.
        """
        ...

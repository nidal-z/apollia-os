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
from apollia.stubs.notify import NotifyInterface
from apollia.stubs.stt import SttInterface
from apollia.stubs.tools import ToolProxy


class StepBudgetView:
    """Read-only snapshot of the execution budget exposed via ``ctx.step_budget``.

    Mirrors ``PyStepBudgetView`` defined in ``crates/apollia-aip/src/context.rs``.
    """

    steps_remaining: int = 0
    """Steps remaining before the ``max_steps`` limit is reached."""

    tool_calls_remaining: int = -1
    """Tool calls remaining before the ``max_tool_calls`` limit is reached."""

    elapsed_seconds: float = 0.0
    """Seconds elapsed since the task started."""


class WorkspaceContextPy:
    """Workspace context collected at task startup.

    Populated by the bridge during ``call_run()``.  Exposes project rules,
    Git state, and custom workspace sections collected by Python providers.
    """

    @property
    def rules(self) -> str | None:
        """Project rules section content (APOLLIA.md / .apollia/rules.md)."""
        ...

    @property
    def apollia_md(self) -> str | None:
        """Alias for ``rules``."""
        ...

    def get(self, title: str) -> str | None:
        """Retrieve a workspace section by title.  Returns ``None`` if absent."""
        ...

    @property
    def sections(self) -> list[dict[str, str]]:
        """All collected sections as ``[{"title": str, "content": str}, ...]``."""
        ...


class RuntimeContext:
    """Execution context injected by the Apollia runtime into ``run(task, ctx)``.

    Exposes optional capabilities: tools, LLM, memory, workspace, and
    inter-agent messaging.  Each property returns ``None`` when the
    capability is not configured for this agent.
    """

    # --- Core capabilities ---

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

    @property
    def workspace(self) -> WorkspaceContextPy | None:
        """Workspace context (rules, Git, custom sections) — ``None`` if not collected."""
        ...

    @property
    def notify(self) -> NotifyInterface | None:
        """Notification interface — ``None`` if no notification channels configured."""
        ...

    @property
    def stt(self) -> SttInterface | None:
        """STT interface — ``None`` if no STT backend configured."""
        ...

    @property
    def user_context(self) -> dict[str, list[tuple[str, str]]] | None:
        """User memory injected in chat mode — ``None`` in task mode.

        Keys: ``"preferences"``, ``"habits"``, ``"context"``.
        Each value is a list of ``(key, value)`` tuples.
        """
        ...

    # --- Budget & logging ---

    @property
    def step_budget(self) -> "StepBudgetView | None":
        """Current execution budget snapshot — ``None`` if not configured."""
        ...

    def log(self, level: str, message: str) -> None:
        """Emit a structured log line from within the agent.

        ``level`` must be one of ``"debug"``, ``"info"``, ``"warn"``, ``"error"``.
        Forwarded to the runtime ``tracing`` layer and surfaced in the operator UI.
        """
        ...

    # --- A2A properties ---

    @property
    def a2a_depth(self) -> int:
        """Current depth in the A2A invocation chain (0 = direct, not via A2A)."""
        ...

    @property
    def user_memory_read_only(self) -> bool:
        """``True`` when invoked via A2A — global user memory is read-only."""
        ...

    # --- A2A messaging ---

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

    # --- A2A delegation ---

    def delegate(
        self,
        skill_id: str,
        payload: dict[str, object],
        timeout_secs: int | None = None,
    ) -> Awaitable[dict[str, object]]:
        """Delegate a task to a Worker Agent by skill ID (low-level).

        The runtime resolves the skill to an active agent, submits the task,
        and waits for the result.  ``timeout_secs`` defaults to 120.

        Returns the task result as a dict.  Raises ``RuntimeError`` if
        ``supports_a2a`` is not enabled, the skill is not found, the skill is
        ambiguous, or the delegation times out.
        """
        ...

    def a2a_invoke(
        self,
        skill_id: str,
        input: dict[str, object],
        timeout_secs: int | None = None,
    ) -> Awaitable[dict[str, object]]:
        """Invoke a Worker Agent by skill ID via the high-level A2A invoker.

        Unlike ``delegate()``, this method never raises Python exceptions on
        agent-level errors — it returns a failed ``AIPResult`` dict instead.

        Returns a dict with keys ``result``, ``agent_name``, ``skill_id``,
        ``duration_ms`` on success, or an ``AIPResult`` failure dict on error.
        """
        ...

    def a2a_discover(
        self,
        skill_id: str,
    ) -> Awaitable[dict[str, object] | None]:
        """Discover the agent that exposes ``skill_id``.

        Returns a discovery card dict, or ``None`` if no agent declares
        the skill.
        """
        ...

    def a2a_list_skills(self) -> Awaitable[list[dict[str, object]]]:
        """List all A2A skills available in the runtime.

        Returns a list of dicts with keys ``skill_id``, ``agent_name``,
        ``skill_name``, ``description``.
        """
        ...

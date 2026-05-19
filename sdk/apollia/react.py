"""ReAct loop utility — Reason + Act loop using ``ctx.llm`` and tools.

ADR-098 retains the ReAct pattern as a free utility (not a base class).
Agents opt in by importing :func:`react` and calling it from any handler::

    from apollia import react


    @agent(name="director", supports_a2a=True)
    class Director:
        @on_message
        async def chat(self, msg, history, ctx):
            return await react(
                ctx,
                system="You are a director agent...",
                user=msg,
                tools=[
                    await ctx.a2a.skill_as_tool("pdf.read_text"),
                    await ctx.a2a.skill_as_tool("web.search"),
                ],
                max_steps=10,
            )

The implementation is intentionally thin: it delegates the actual
reasoning + tool-call loop to the Rust-side ``LlmProxy.run_tools`` helper
(see ``crates/apollia-aip/src/llm.rs``), which already handles the
``LLM -> tool(s) -> LLM -> ... -> final answer`` cycle natively. The
``react`` helper simply assembles the message list, enforces an explicit
``max_steps`` budget, and surfaces a typed :class:`DomainError` when
``max_steps <= 0`` so agent code can branch on the failure mode.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from apollia.errors import DomainError

if TYPE_CHECKING:
    from apollia.types import Ctx


async def react(
    ctx: Ctx,
    system: str,
    user: str,
    *,
    tools: list[dict[str, Any]] | None = None,
    max_steps: int = 15,
    temperature: float = 0.3,
    stream: bool = True,
) -> str:
    """Execute a ReAct loop with ``ctx.llm`` and the provided tools.

    Args:
        ctx: Runtime context (must expose ``ctx.llm`` with ``run_tools``).
        system: System prompt.
        user: Initial user message.
        tools: List of tool descriptors compatible with Anthropic /
            OpenAI tool-use (``{"name", "description", "input_schema"}``).
            Build them via ``ctx.a2a.skill_as_tool(skill_id)`` for A2A
            workers, or ``ctx.tools.describe(name)`` for native tools.
            ``None`` is treated as no tools (pure chat).
        max_steps: Maximum LLM round-trips. Must be ``>= 1``.
        temperature: LLM sampling temperature (currently informational;
            ``LlmProxy.run_tools`` uses the backend default).
        stream: If ``True`` and ``ctx.events`` is available, emit a
            single ``emit_thought`` marker per call to surface the ReAct
            step in observability streams. Real token-level streaming
            stays inside the LLM router and is not duplicated here.

    Returns:
        The final assistant answer as a plain string.

    Raises:
        DomainError: With code ``"REACT_MAX_STEPS"`` if ``max_steps <= 0``.
            (The internal LLM loop raises its own error if it exhausts
            ``max_steps`` without converging — that error bubbles up
            unchanged.)
    """
    if max_steps <= 0:
        raise DomainError(
            "REACT_MAX_STEPS",
            f"react requires max_steps >= 1, got {max_steps}",
        )

    # Best-effort observability: surface the entry into the ReAct loop
    # so dashboards can correlate it with subsequent tool calls.
    if stream:
        events = getattr(ctx, "events", None)
        if events is not None:
            emit_thought = getattr(events, "emit_thought", None)
            if callable(emit_thought):
                try:
                    emit_thought(
                        f"react: entering loop (max_steps={max_steps})",
                        step=0,
                    )
                except Exception:
                    # Observability must never break the agent run.
                    pass

    messages: list[dict[str, Any]] = [
        {"role": "system", "content": system},
        {"role": "user", "content": user},
    ]
    tool_list: list[dict[str, Any]] = list(tools) if tools else []

    result = await ctx.llm.run_tools(  # type: ignore[attr-defined]
        messages,
        tool_list,
        max_iterations=max_steps,
    )
    return str(result)

"""ConversationalAgent — Base class for dialogue-only agents on Apollia OS.

Manages conversation history, LLM calls via ``ctx.llm.complete()``, and
optional memory persistence.  Subclasses define ``SYSTEM_PROMPT`` and
implement ``manifest()``.  Override ``on_response()`` for post-processing.

Usage::

    class GreeterAgent(ConversationalAgent):
        SYSTEM_PROMPT = "You are a friendly greeter."

        def manifest(self):
            return {
                "name": "greeter",
                "version": "0.1.0",
                "execution_mode": "direct",
                "tools_required": [],
            }

    # The runtime calls agent.run(task, ctx) automatically.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Any

from apollia.types import AIPResult


class ConversationalAgent(ABC):
    """Base class for conversational agents (no tools, pure dialogue).

    Subclasses must define:
    - SYSTEM_PROMPT: str — the system prompt for the LLM
    - manifest() -> dict — agent metadata

    Optional overrides:
    - on_response(response: str) -> str — post-process LLM response
    - MAX_TURNS: int — maximum conversation turns (default 20)
    - TEMPERATURE: float — LLM temperature (default 0.7)
    """

    SYSTEM_PROMPT: str = ""
    MAX_TURNS: int = 20
    TEMPERATURE: float = 0.7

    @abstractmethod
    def manifest(self) -> dict[str, Any]:
        """Return agent manifest dict for runtime registration."""
        ...

    def on_response(self, response: str) -> str:
        """Post-process LLM response. Override for custom behavior."""
        return response

    async def converse(
        self,
        ctx: Any,
        user_message: str,
        history: list[dict[str, str]] | None = None,
    ) -> tuple[str, list[dict[str, str]]]:
        """Send a message and get a response, maintaining conversation history.

        Args:
            ctx: RuntimeContext injected by the Apollia runtime.
            user_message: The user's message.
            history: Optional existing conversation history.

        Returns:
            Tuple of (assistant_response, updated_history).

        Raises:
            RuntimeError: If ctx.llm is None.
        """
        if ctx.llm is None:
            raise RuntimeError(
                "ConversationalAgent requires ctx.llm"
                " — no LLM backend configured"
            )

        messages: list[dict[str, str]] = list(history) if history else []

        if not messages or messages[0].get("role") != "system":
            messages.insert(0, {"role": "system", "content": self.SYSTEM_PROMPT})

        messages.append({"role": "user", "content": user_message})

        response = await ctx.llm.complete(messages)
        assistant_text: str = response.get("text", "")
        processed_text = self.on_response(assistant_text)

        messages.append({"role": "assistant", "content": processed_text})

        if ctx.memory is not None:
            await ctx.memory.record(
                content=f"user: {user_message}\nassistant: {processed_text}",
                importance=0.3,
            )

        return processed_text, messages

    async def run(self, task: Any, ctx: Any) -> AIPResult:
        """Execute the conversational agent.

        Extracts user message from task.input.text, calls converse(),
        and returns the response as AIPResult.completed().
        """
        if ctx.llm is None:
            raise RuntimeError(
                "ConversationalAgent requires ctx.llm"
                " — no LLM backend configured"
            )

        user_message = getattr(task, "input", None)
        if user_message is None:
            return AIPResult.failed("NO_INPUT", "No input provided in task")

        input_text = (
            user_message.text
            if hasattr(user_message, "text")
            else str(user_message)
        )

        response_text, _ = await self.converse(ctx, input_text)
        return AIPResult.completed(response_text)

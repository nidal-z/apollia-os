"""Minimal no-LLM agent used by the host-SDK demo.

Echoes its input so the demo can drive a real Apollia daemon end-to-end over
TCP + bearer token without requiring a local model. It exercises the same
submit / poll path a host integration uses, only with a deterministic body.
"""

from apollia import agent, on_message
from apollia.types import Ctx, Message


@agent(
    name="echo",
    version="0.1.0",
    description="Echoes the incoming message. Used by the host-SDK demo.",
)
class Echo:
    """Conversational agent that returns its input verbatim, no LLM call."""

    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        ctx.log("info", "echo.received")
        return f"echo: {message}"


agent = Echo()  # type: ignore[assignment]

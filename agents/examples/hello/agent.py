"""Minimal Apollia OS example agent.

This is the smallest possible agent. It listens for chat messages and echoes
them back unchanged. Use it as a smoke test to confirm your local Apollia
installation is wired up, and as a starting point for your own agents.
"""

from apollia import agent, on_message
from apollia.context import Ctx


@agent(
    name="hello",
    description="Echoes back whatever message you send.",
)
class Hello:
    @on_message
    async def handle(self, message: str, history: list, ctx: Ctx) -> str:
        ctx.logger.info("hello agent received a message", length=len(message))
        return f"You said: {message}"


agent = Hello()

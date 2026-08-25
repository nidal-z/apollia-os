"""Minimal Apollia OS example agent.

The smallest agent that does something: it listens for chat messages and echoes
them back. Use it to confirm a local install is wired up, and as the starting
point for your own agent.

The whole contract is visible here: a class carrying `@agent` and one async
`@on_message` method. That is all of it.
"""

from apollia import agent, on_message
from apollia.types import Ctx


@agent(
    name="hello",
    version="0.1.0-preview",
    description="Echoes back whatever message you send.",
)
class Hello:
    """Echo agent: the minimal contract, one class and one handler."""

    @on_message
    async def handle(self, message: str, history: list[dict], ctx: Ctx) -> str:
        """Echo the incoming message back to the sender."""
        # `ctx.logger` is a stdlib `logging.Logger` routed into the runtime
        # tracer, so it takes printf-style arguments. Passing structured
        # keyword fields raises TypeError at the first message.
        ctx.logger.info("hello agent received %d characters", len(message))
        return f"You said: {message}"


# No module-level `agent = Hello()` here, and that is deliberate: `@agent`
# instantiates the class and binds the instance to this module itself. Writing
# it by hand builds a second instance that overwrites the registered one.

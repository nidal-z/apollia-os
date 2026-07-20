"""apollia-chat, seed stub for the automation verification suite.

A minimal but VALID Apollia agent: the boot auto-loader imports it and requires
the ``@agent`` decorator (which caches ``__apollia_manifest__``), so the old
``manifest()``/``run()`` shape does not load. It never answers a real turn during
the deterministic suite (no model), it only has to register cleanly so the
free-chat agent surface resolves.
"""

from apollia import agent, on_message
from apollia.types import Ctx, Message


@agent(
    name="apollia-chat",
    version="1.0.0",
    description="Apollia free-chat assistant.",
    tags=("chat", "assistant", "system"),
    memory_namespace="apollia-chat",
    agent_type="system",
)
class ApolliaChat:
    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        return "Seed free-chat stub."


agent = ApolliaChat()

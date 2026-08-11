"""apollia-guide, seed stub for the automation verification suite.

A minimal but VALID Apollia agent: the boot auto-loader imports it and requires
the ``@agent`` decorator (which caches ``__apollia_manifest__``), so the old
``manifest()``/``run()`` shape does not load. It never answers a real turn during
the deterministic suite (no model), it only has to register cleanly so the agent
surfaces (detail tabs, chat agent picker, coach) resolve.
"""

from apollia import agent, on_message
from apollia.types import Ctx, Message


@agent(
    name="apollia-guide",
    version="0.1.0-preview",
    description=(
        "Conversational coach for Apollia OS: knows the product's real "
        "capabilities and suggests actionable deep-links."
    ),
    tags=("coach", "system", "guide", "meta"),
    memory_namespace="apollia-guide",
    agent_type="system",
)
class ApolliaGuide:
    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        return "Seed coach stub."


agent = ApolliaGuide()

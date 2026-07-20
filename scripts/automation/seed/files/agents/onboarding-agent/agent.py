"""onboarding-agent, seed stub for the automation verification suite.

A minimal but VALID Apollia agent: the boot auto-loader imports it and requires
the ``@agent`` decorator (which caches ``__apollia_manifest__``), so the old
``manifest()``/``run()`` shape does not load. It never answers a real turn during
the deterministic suite (no model), it only has to register cleanly.
"""

from apollia import agent, on_message
from apollia.types import Ctx, Message


@agent(
    name="onboarding-agent",
    version="2.4.0",
    description="First user contact calibration.",
    tags=("onboarding", "system"),
    memory_namespace="onboarding",
    agent_type="system",
)
class OnboardingAgent:
    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        return "Seed onboarding stub."


agent = OnboardingAgent()

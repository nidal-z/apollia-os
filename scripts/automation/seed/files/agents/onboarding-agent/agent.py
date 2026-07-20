"""onboarding-agent - seed stub for automation verification.

Minimal, self-contained module. Not loaded by the runtime during the automation
suite; present so the install directory resolves on disk.
"""


class OnboardingAgent:
    """First-contact calibration surrogate."""

    def manifest(self) -> dict:
        return {
            "name": "onboarding-agent",
            "version": "2.4.0",
            "description": "First user contact calibration.",
            "tools_required": ["permission_rule_add", "permission_rule_list"],
            "agent_type": "system",
            "execution_mode": "conversational",
            "memory_namespace": "onboarding",
        }

    async def run(self, ctx, task):
        return {"ok": True}


agent = OnboardingAgent()

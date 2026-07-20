"""apollia-guide - seed stub for automation verification.

Minimal, self-contained module. Not loaded by the runtime during the automation
suite; present so the install directory (with its knowledge base) resolves.
"""


class ApolliaGuide:
    """Product coach surrogate."""

    def manifest(self) -> dict:
        return {
            "name": "apollia-guide",
            "version": "0.2.0",
            "description": "Conversational coach for Apollia OS.",
            "tools_required": [],
            "tools_optional": [
                "navigate",
                "read_memory_namespace",
                "get_user_integrations",
                "get_installed_agents",
            ],
            "agent_type": "system",
            "execution_mode": "conversational",
            "memory_namespace": "apollia-guide",
        }

    async def run(self, ctx, task):
        return {"ok": True}


agent = ApolliaGuide()

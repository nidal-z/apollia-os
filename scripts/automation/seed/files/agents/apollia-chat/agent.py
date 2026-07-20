"""apollia-chat - seed stub for automation verification.

Minimal, self-contained module. Never loaded by the runtime during the
automation suite: it exists so the install directory resolves on disk and the
agents view can display an installed agent with a valid manifest.
"""


class ApolliaChat:
    """Free-chat assistant surrogate."""

    def manifest(self) -> dict:
        return {
            "name": "apollia-chat",
            "version": "1.0.0",
            "description": "Apollia free-chat assistant.",
            "tools_required": [],
            "tools_optional": ["web_search", "file_read"],
            "agent_type": "system",
            "execution_mode": "auto",
            "memory_namespace": "apollia-chat",
        }

    async def run(self, ctx, task):
        return {"ok": True}


agent = ApolliaChat()

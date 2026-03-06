"""Minimal AIP-compatible demo agent.

Demonstrates the simplest possible Apollia agent: manifest() + async run().
No tools, no memory, no external dependencies.
"""


class HelloAgent:
    """Echoes back user input with a greeting."""

    def manifest(self):
        return {
            "name": "hello-agent",
            "version": "1.0.0",
            "description": "Agent de demonstration minimal",
            "tools_required": [],
            "max_concurrent_tasks": 1,
        }

    async def run(self, task, ctx):
        parts = task.get("input", {}).get("parts", [])
        text = parts[0]["text"] if parts else "monde"
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": f"Bonjour ! J'ai recu : {text}"}],
        }


agent = HelloAgent()

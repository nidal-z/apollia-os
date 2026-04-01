"""generic-agent — Baseline ReAct agent without domain expertise.

Used as the comparison baseline in Worker Agent benchmarks.
Provides no domain-specific system prompt, guardrails, or compiled patterns —
the LLM must discover correct tool usage on its own from its pre-training.
"""
from __future__ import annotations

from typing import Any

from apollia.agents import AIPResult, BaseReActAgent

SYSTEM_PROMPT: str = """Tu es un assistant IA généraliste capable d'aider avec des tâches variées.

Tu as accès aux outils suivants pour accomplir tes tâches :
- python_executor : exécute du code Python
- file_read : lit le contenu d'un fichier
- file_write : écrit du contenu dans un fichier
- bash_executor : exécute des commandes shell
- file_list : liste les fichiers d'un répertoire

Utilise ces outils pour accomplir la tâche demandée.
Réponds de manière concise et précise en français.
"""


class GenericAgent(BaseReActAgent):
    """Generic ReAct agent with no domain-specific expertise.

    Acts as the baseline comparison in Worker Agent benchmarks. Has no
    pre-compiled knowledge of correct patterns for Excel files, encoding
    handling, or tool usage best practices.
    """

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 10
    TEMPERATURE = 0.3

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for generic-agent."""
        return {
            "name": "generic-agent",
            "version": "0.1.0",
            "description": (
                "Agent généraliste sans expertise domaine. "
                "Utilisé comme baseline dans les benchmarks Worker Agent."
            ),
            "execution_mode": "direct",
            "tools_required": ["python_executor", "file_read", "file_write"],
            "tools_optional": ["bash_executor", "file_list"],
            "tools_requiring_approval": [],
            "packages": [],
            "memory_namespace": "generic-agent",
            "supports_a2a": False,
            "skills": [],
            "tags": ["generic", "baseline"],
            "max_concurrent_tasks": 1,
            "dangerous_tools_allowed": False,
        }

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Execute the task using the ReAct loop."""
        user_message = (
            task.get("input", {}).get("text", "")
            if isinstance(task.get("input"), dict)
            else str(task.get("input", ""))
        )
        result = await self.react(task, ctx, user_message)
        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)


agent = GenericAgent()

"""OrchestratedAgent — Base class for ORIA-piloted agents on Apollia OS.

In orchestrated mode (ADR-022 Option B), ORIA generates an
``ExecutionPlan``, executes tools directly, and calls
``on_plan_complete(step_results)`` on the agent once the plan finishes.

The agent provides only metadata (``manifest()``) and optional
post-processing (``on_plan_complete()``).  ``run()`` is never called by
the runtime in this mode — it raises ``RuntimeError`` if invoked.

Usage::

    class AnalyzerAgent(OrchestratedAgent):
        def manifest(self):
            return {
                "name": "analyzer",
                "version": "0.1.0",
                "execution_mode": "orchestrated",
                "system_prompt": "Analyze data using available tools.",
                "tools_required": ["bash", "file_io"],
            }

        def on_plan_complete(self, step_results):
            # Custom post-processing
            return {"text": summarize(step_results)}
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import Any

from apollia.types import AIPResult


class OrchestratedAgent(ABC):
    """Base class for ORIA-orchestrated agents.

    In orchestrated mode, ORIA generates and executes the plan.
    The agent only provides metadata and optional post-processing.

    Subclasses must define:
    - manifest() -> dict: must include execution_mode='orchestrated' and system_prompt

    Optional overrides:
    - on_plan_complete(step_results: dict) -> dict: post-process plan results
    """

    @abstractmethod
    def manifest(self) -> dict[str, Any]:
        """Return agent manifest. Must include execution_mode='orchestrated' and system_prompt."""
        ...

    async def run(self, task: Any, ctx: Any) -> AIPResult:
        """Orchestrated agents should not have run() called directly.

        Raises:
            RuntimeError: Always. ORIA handles execution for orchestrated agents.
        """
        raise RuntimeError(
            "This agent is orchestrated — run() should not be called directly. "
            "Set execution_mode='orchestrated' in manifest."
        )

    def on_plan_complete(self, step_results: dict[str, Any]) -> dict[str, Any]:
        """Post-process step results after ORIA plan execution.

        Default behavior: auto-concatenate text outputs from all steps.
        Override for custom post-processing.

        Args:
            step_results: Dict mapping step_id to step result dict.

        Returns:
            Processed results dict with at least a 'text' key.
        """
        texts = []
        for _step_id, result in step_results.items():
            if isinstance(result, dict) and "text" in result:
                texts.append(result["text"])
            elif isinstance(result, str):
                texts.append(result)
        return {"text": "\n\n".join(texts)}

    @staticmethod
    def format_step_results(results: dict[str, Any]) -> str:
        """Format step results as a human-readable string.

        Args:
            results: Dict mapping step_id to step result.

        Returns:
            Formatted multi-line string.
        """
        lines = []
        for step_id, result in results.items():
            if isinstance(result, dict):
                text = result.get("text", result.get("output", str(result)))
                status = result.get("status", "unknown")
                lines.append(f"[{step_id}] ({status}) {text}")
            else:
                lines.append(f"[{step_id}] {result}")
        return "\n".join(lines)

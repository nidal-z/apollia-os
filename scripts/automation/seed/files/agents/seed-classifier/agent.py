"""seed-classifier, seed A2A worker stub for the automation verification suite.

A minimal but VALID Apollia worker: the boot auto-loader imports it and requires
the ``@agent`` decorator (which caches ``__apollia_manifest__``), so the old
``manifest()`` shape does not load. It exposes one ``@skill`` so the agent
registers with a discoverable A2A surface. It never runs a real classification
during the deterministic suite (no model); it returns the first label
deterministically.
"""

from apollia import agent, skill
from apollia.types import Ctx


@agent(
    name="seed-classifier",
    version="0.1.0",
    description="Deterministic text classifier worker.",
    tags=("worker", "classifier", "seed"),
    memory_namespace="seed-classifier",
    agent_type="worker",
)
class SeedClassifier:
    @skill(
        "classify_text",
        description="Classify a text into one of the provided labels.",
    )
    async def classify_text(
        self,
        text: str,
        labels: list[str],
        ctx: Ctx = None,  # type: ignore[assignment]
    ) -> dict:
        """Return the first candidate label (deterministic seed behaviour)."""
        return {"label": labels[0] if labels else None, "text": text}


agent = SeedClassifier()

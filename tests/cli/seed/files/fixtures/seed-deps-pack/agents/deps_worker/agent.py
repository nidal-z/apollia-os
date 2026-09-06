"""seed-deps-pack / seed-deps-worker, packaged worker stub for the install dialog fixtures.

Never started by the seed: the package sits under fixtures/ so the dialog can
preview it from a picker the automaton answers. Same VALID shape as the
seed classifier: the loader requires the ``@agent`` decorator (which caches
``__apollia_manifest__``), so the old ``manifest()`` shape does not install.
"""

from apollia import agent, skill
from apollia.types import Ctx


@agent(
    name="seed-deps-worker",
    version="0.1.0",
    description="Seed fixture worker declaring a pip dependency.",
    tags=("worker", "seed", "fixture"),
    memory_namespace="seed-deps-worker",
    agent_type="worker",
)
class SeedDepsWorker:
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
        """Return the first candidate label (deterministic fixture behaviour)."""
        return {"label": labels[0] if labels else None, "text": text}


agent = SeedDepsWorker()

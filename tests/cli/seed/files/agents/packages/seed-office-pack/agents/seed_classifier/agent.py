"""seed-office-pack / seed-classifier - packaged worker stub.

Self-contained copy of the classifier worker inside the package layout, so the
package root resolves on disk (root_missing = false in the packages view).
"""


class SeedClassifier:
    """Single-skill classifier surrogate."""

    def manifest(self) -> dict:
        return {
            "name": "seed-classifier",
            "version": "0.1.0",
            "description": "Deterministic text classifier worker.",
            "tools_required": [],
            "agent_type": "worker",
            "supports_a2a": True,
        }

    async def classify_text(self, ctx, payload):
        labels = payload.get("labels", [])
        return {"label": labels[0] if labels else None}


agent = SeedClassifier()

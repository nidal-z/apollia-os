"""seed-classifier - seed worker stub for automation verification.

Minimal, self-contained A2A worker surrogate. Not loaded by the runtime during
the automation suite; present so the install directory resolves on disk.
"""


class SeedClassifier:
    """Single-skill classifier surrogate."""

    def manifest(self) -> dict:
        return {
            "name": "seed-classifier",
            "version": "0.1.0",
            "description": "Deterministic text classifier worker.",
            "tools_required": [],
            "tools_optional": ["llm"],
            "agent_type": "worker",
            "execution_mode": "direct",
            "supports_a2a": True,
            "memory_namespace": "seed-classifier",
            "skills": [
                {
                    "id": "classify-text",
                    "name": "Classify text",
                    "description": "Classify a text into one of the provided labels.",
                }
            ],
        }

    async def classify_text(self, ctx, payload):
        labels = payload.get("labels", [])
        return {"label": labels[0] if labels else None}


agent = SeedClassifier()

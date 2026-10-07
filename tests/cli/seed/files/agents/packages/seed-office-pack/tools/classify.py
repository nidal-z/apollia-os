"""The tool of the seed package seed-office-pack.

Deterministic: the label whose name the text carries, else the first one. No
model, no network, so the suite can call it without either.
"""

from typing import Annotated, Any

from apollia import tool
from apollia.types import Ctx


@tool(
    "classify_text",
    "Classify a text into one of the provided labels.",
    action="read",
)
async def classify_text(
    ctx: Ctx,
    text: Annotated[str, "Text to classify."],
    labels: Annotated[list[str], "Candidate labels."],
) -> dict[str, Any]:
    ctx.logger.info("classify_text called")
    lowered = text.lower()
    for label in labels:
        if label.lower() in lowered:
            return {"label": label}
    return {"label": labels[0] if labels else None}

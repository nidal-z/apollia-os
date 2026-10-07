"""The tool of the package word-count.

The package declares no agent. Its tool is granted to an agent of the
installation, under the name ``pkg:word-count/count_words``.
"""

from typing import Annotated

from apollia import tool
from apollia.types import Ctx


@tool(
    "count_words",
    "Count the words, the lines and the characters of a text.",
    action="read",
    examples=[{"text": "A short note.\nOn two lines."}],
)
async def count_words(
    ctx: Ctx,
    text: Annotated[str, "The text to measure."],
) -> dict[str, int]:
    """Return the number of words, lines and characters of ``text``."""
    ctx.logger.info("count_words called on %d characters", len(text))
    return {
        "words": len(text.split()),
        "lines": len(text.splitlines()),
        "characters": len(text),
    }
